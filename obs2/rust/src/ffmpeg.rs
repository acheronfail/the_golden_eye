//! Thin wrapper over the statically-linked FFmpeg (libav*) for the one video
//! operation the recorder needs: trimming a saved replay-buffer file down to the
//! clip we actually want to keep.
//!
//! The trim is a pure remux -- packets are stream-copied, never re-encoded -- so
//! it is fast and lossless. Stream copy can only cut on keyframe boundaries, so
//! the kept clip may start a keyframe or two ahead of the requested point; that
//! is an accepted trade-off for not transcoding (GoldenEye recordings have
//! frequent keyframes, so the slack is small).

use std::path::Path;

use anyhow::{Context, anyhow};
use ffmpeg_next::ffi::AV_TIME_BASE;
use ffmpeg_next::{Rescale, codec, encoder, format, media, rescale};

/// Initialise FFmpeg. Cheap and safe to call repeatedly (libav guards its own
/// one-time setup), so each entry point calls it rather than relying on a
/// caller to have done so.
fn init() -> anyhow::Result<()> {
    ffmpeg_next::init().map_err(|e| anyhow!("ffmpeg init failed: {e}"))
}

/// Container duration of `path` in seconds, as reported by the demuxer.
pub fn duration_secs(path: &Path) -> anyhow::Result<f64> {
    init()?;
    let ictx = format::input(path).with_context(|| format!("opening {}", path.display()))?;
    // `duration()` is in AV_TIME_BASE (microsecond) units.
    Ok(ictx.duration() as f64 / AV_TIME_BASE as f64)
}

/// Remux `input` into `output`, keeping only the packets between `start_secs`
/// and `end_secs` (both measured from the start of `input`). Packets are
/// stream-copied, so this neither re-encodes nor decodes; the output container
/// format is inferred from `output`'s extension.
///
/// Timestamps are shifted so the kept clip begins at ~0 in the output (using a
/// single global offset, so audio and video stay in sync). Because the cut is
/// on keyframe boundaries, the real start may be slightly before `start_secs`.
pub fn trim(input: &Path, output: &Path, start_secs: f64, end_secs: f64) -> anyhow::Result<()> {
    init()?;

    let mut ictx = format::input(input).with_context(|| format!("opening {}", input.display()))?;
    let mut octx = format::output(output).with_context(|| format!("creating {}", output.display()))?;

    // Map each input stream we keep (audio/video/subtitle) to an output stream,
    // remembering its input time base (the iterator borrows `ictx`, so we can't
    // re-read it once we're pulling packets).
    let n = ictx.nb_streams() as usize;
    let mut stream_mapping: Vec<i32> = vec![-1; n];
    let mut ist_time_bases = vec![ffmpeg_next::Rational(0, 1); n];
    let mut ost_index: i32 = 0;
    for (ist_index, ist) in ictx.streams().enumerate() {
        let medium = ist.parameters().medium();
        if medium != media::Type::Audio && medium != media::Type::Video && medium != media::Type::Subtitle {
            continue;
        }
        stream_mapping[ist_index] = ost_index;
        ist_time_bases[ist_index] = ist.time_base();
        ost_index += 1;

        let mut ost = octx.add_stream(encoder::find(codec::Id::None))?;
        ost.set_parameters(ist.parameters());
        // Clear the codec tag so muxing into a different container doesn't trip
        // an "incompatible codec tag" error (per the ffmpeg-next remux example).
        unsafe {
            (*ost.parameters().as_mut_ptr()).codec_tag = 0;
        }
    }

    let mapped = stream_mapping.iter().filter(|&&m| m >= 0).count();
    if mapped == 0 {
        return Err(anyhow!("no audio/video/subtitle streams to copy from {}", input.display()));
    }

    octx.set_metadata(ictx.metadata().to_owned());
    octx.write_header().context("writing output header")?;

    // Seek to (or just before) the start so we don't decode the whole file; the
    // demuxer lands on the nearest keyframe at or before this point.
    let start_avtb = (start_secs.max(0.0) * AV_TIME_BASE as f64) as i64;
    if start_avtb > 0 {
        // Ignore seek failures: a tiny/keyframe-less file just plays from 0.
        let _ = ictx.seek(start_avtb, ..start_avtb);
    }

    // Shift a timestamp from the input stream's time base into the output
    // stream's time base, with the global start offset subtracted so the clip
    // begins at ~0. A keyframe slightly before the cut clamps to 0 rather than
    // going negative (muxers like mov reject negative timestamps).
    let shift = |t: i64, in_tb: ffmpeg_next::Rational, out_tb: ffmpeg_next::Rational| -> i64 {
        let avtb = t.rescale(in_tb, rescale::TIME_BASE) - start_avtb;
        avtb.max(0).rescale(rescale::TIME_BASE, out_tb)
    };

    // Track which kept streams have run past `end_secs`; stop once all have, so
    // we copy every stream right up to the cut without reading the whole file.
    let mut finished = vec![false; n];
    let mut done = 0usize;

    for (stream, mut packet) in ictx.packets() {
        let ist_index = stream.index();
        let ost_index = stream_mapping[ist_index];
        if ost_index < 0 {
            continue;
        }

        let in_tb = ist_time_bases[ist_index];
        // Packet time in seconds, for the end-bound check.
        let ts = packet.dts().or_else(|| packet.pts());
        if let Some(ts) = ts {
            let secs = ts as f64 * in_tb.numerator() as f64 / in_tb.denominator() as f64;
            if secs > end_secs {
                if !finished[ist_index] {
                    finished[ist_index] = true;
                    done += 1;
                    if done >= mapped {
                        break;
                    }
                }
                continue;
            }
        }

        let out_tb = octx.stream(ost_index as usize).expect("output stream exists").time_base();
        packet.set_pts(packet.pts().map(|t| shift(t, in_tb, out_tb)));
        packet.set_dts(packet.dts().map(|t| shift(t, in_tb, out_tb)));
        packet.set_position(-1);
        packet.set_stream(ost_index as usize);
        packet.write_interleaved(&mut octx).context("writing packet")?;
    }

    octx.write_trailer().context("writing output trailer")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A short sample clip lives at the repository root; used to exercise the
    // trim path end-to-end against a real container.
    fn sample_clip() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample_clip.mov")
    }

    #[test]
    fn reads_duration() {
        let dur = duration_secs(&sample_clip()).expect("probe duration");
        assert!(dur > 1.0, "sample clip should be longer than a second, got {dur}");
    }

    #[test]
    fn trims_to_requested_window() {
        let input = sample_clip();
        let full = duration_secs(&input).expect("probe duration");

        // Trim a window comfortably inside the clip and confirm the output is a
        // valid container of roughly the requested length. Keyframe-aligned cuts
        // mean the real bounds drift a little, so the tolerance is generous.
        let (start, end) = (1.0, (full - 1.0).max(2.0));
        let want = end - start;

        let out = std::env::temp_dir().join("ge_ffmpeg_trim_test.mov");
        let _ = std::fs::remove_file(&out);
        trim(&input, &out, start, end).expect("trim");

        let got = duration_secs(&out).expect("probe trimmed duration");
        assert!((got - want).abs() < 1.5, "trimmed duration {got:.3}s should be near requested {want:.3}s",);
        let _ = std::fs::remove_file(&out);
    }
}
