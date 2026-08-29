//! Thin wrapper over statically-linked FFmpeg for the one op the recorder needs:
//! trimming a saved replay-buffer file to the wanted clip. It's a pure stream-copy
//! remux (fast, lossless), so cuts land on keyframes -- the clip may start early.

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use ffmpeg_next::ffi::AV_TIME_BASE;
use ffmpeg_next::{Dictionary, DictionaryRef, Rescale, codec, encoder, format, media, rescale};

pub use crate::models::clip_metadata::ClipMetadata;
use crate::models::clip_metadata::is_ffmpeg_plugin_tag;

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

/// Reads the plugin metadata from `path`. Returns `Ok(None)` for a readable
/// video/container that was not created by The Golden Eye.
pub fn read_clip_metadata(path: &Path) -> anyhow::Result<Option<ClipMetadata>> {
    init()?;
    let ictx = format::input(path).with_context(|| format!("opening {}", path.display()))?;
    Ok(ClipMetadata::from_ffmpeg_tags(&ictx.metadata()))
}

/// Rewrites only the container metadata for `path`, preserving streams without
/// re-encoding. The update is staged in the same directory and swapped into
/// place after a successful remux.
pub fn rewrite_metadata_in_place(path: &Path, clip_metadata: &ClipMetadata) -> anyhow::Result<()> {
    let temp = sibling_temp_path(path, "metadata")?;
    let backup = sibling_temp_path(path, "metadata-backup")?;

    let result = (|| {
        rewrite_metadata(path, &temp, clip_metadata)?;
        replace_file_with_backup(path, &temp, &backup)
    })();

    if result.is_err()
        && let Err(err) = std::fs::remove_file(&temp)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %temp.display(), "failed to remove incomplete metadata rewrite: {err}");
    }

    result
}

/// Remuxes `input` to `output` with updated plugin metadata, preserving all
/// copied streams and non-plugin metadata.
#[cfg_attr(not(test), allow(dead_code))]
pub fn rewrite_metadata(input: &Path, output: &Path, clip_metadata: &ClipMetadata) -> anyhow::Result<()> {
    remux_with_metadata(input, output, None, Some(clip_metadata))
}

/// Remux `input` to `output`, keeping only packets between `start_secs` and `end_secs`.
/// Packets are stream-copied (no re-encode/decode); output format from `output`'s ext.
/// Timestamps shift so the clip starts at ~0; keyframe cuts may begin before `start_secs`.
#[cfg(test)]
fn trim(input: &Path, output: &Path, start_secs: f64, end_secs: f64) -> anyhow::Result<()> {
    trim_with_metadata(input, output, start_secs, end_secs, None)
}

pub fn trim_with_metadata(
    input: &Path,
    output: &Path,
    start_secs: f64,
    end_secs: f64,
    clip_metadata: Option<&ClipMetadata>,
) -> anyhow::Result<()> {
    remux_with_metadata(input, output, Some((start_secs, end_secs)), clip_metadata)
}

fn remux_with_metadata(
    input: &Path,
    output: &Path,
    trim_window: Option<(f64, f64)>,
    clip_metadata: Option<&ClipMetadata>,
) -> anyhow::Result<()> {
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

    let metadata = match clip_metadata {
        Some(clip_metadata) => metadata_with_plugin_tags(&ictx.metadata(), clip_metadata),
        None => ictx.metadata().to_owned(),
    };
    octx.set_metadata(metadata);
    write_header(&mut octx, output).context("writing output header")?;

    // Seek to (or just before) the start so we don't decode the whole file; the
    // demuxer lands on the nearest keyframe at or before this point.
    let start_secs = trim_window.map(|(start_secs, _)| start_secs).unwrap_or(0.0);
    let end_secs = trim_window.map(|(_, end_secs)| end_secs);
    let start_avtb = (start_secs.max(0.0) * AV_TIME_BASE as f64) as i64;
    if start_avtb > 0 {
        // Ignore seek failures: a tiny/keyframe-less file just plays from 0.
        let _ = ictx.seek(start_avtb, ..start_avtb);
    }

    // Shift a timestamp from the input to the output stream time base, minus the
    // global start offset so the clip begins at ~0. Clamps to 0 (muxers like mov
    // reject negative timestamps).
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
        if let (Some(end_secs), Some(ts)) = (end_secs, ts) {
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

fn metadata_with_plugin_tags(source: &DictionaryRef<'_>, clip_metadata: &ClipMetadata) -> Dictionary<'static> {
    let mut metadata = Dictionary::new();
    for (key, value) in source.iter() {
        if !is_ffmpeg_plugin_tag(key) {
            metadata.set(key, value);
        }
    }
    clip_metadata.write_ffmpeg_tags(&mut metadata);
    metadata
}

fn sibling_temp_path(path: &Path, role: &str) -> anyhow::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("clip");
    let ext = path.extension().and_then(|ext| ext.to_str());
    let pid = std::process::id();

    for i in 0..1000 {
        let suffix = if i == 0 { format!("{role}-{pid}") } else { format!("{role}-{pid}-{i}") };
        let file_name = match ext {
            Some(ext) if !ext.is_empty() => format!(".{stem}.{suffix}.{ext}"),
            _ => format!(".{stem}.{suffix}"),
        };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(anyhow!("could not allocate temporary path beside {}", path.display()))
}

fn replace_file_with_backup(path: &Path, replacement: &Path, backup: &Path) -> anyhow::Result<()> {
    if let Ok(metadata) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(replacement, metadata.permissions());
    }

    std::fs::rename(path, backup).with_context(|| format!("moving {} to {}", path.display(), backup.display()))?;
    match std::fs::rename(replacement, path) {
        Ok(()) => {
            if let Err(err) = std::fs::remove_file(backup) {
                tracing::warn!(path = %backup.display(), "failed to remove metadata rewrite backup: {err}");
            }
            Ok(())
        }
        Err(err) => {
            let restore = std::fs::rename(backup, path);
            match restore {
                Ok(()) => Err(err).with_context(|| format!("moving {} to {}", replacement.display(), path.display())),
                Err(restore_err) => Err(anyhow!(
                    "moving {} to {} failed ({err}); restoring {} also failed ({restore_err})",
                    replacement.display(),
                    path.display(),
                    backup.display()
                )),
            }
        }
    }
}

fn write_header(octx: &mut format::context::Output, output: &Path) -> anyhow::Result<()> {
    if !needs_mov_metadata_tags(output) {
        octx.write_header()?;
        return Ok(());
    }

    let mut options = Dictionary::new();
    options.set("movflags", "use_metadata_tags");
    let unused = octx.write_header_with(options)?;
    for (key, value) in unused.iter() {
        tracing::debug!(key, value, "FFmpeg muxer returned unused header option");
    }
    Ok(())
}

fn needs_mov_metadata_tags(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "m4v" | "mov" | "3gp" | "3g2"))
}

#[cfg(test)]
#[path = "ffmpeg_test.rs"]
mod ffmpeg_test;
