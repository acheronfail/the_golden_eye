//! Thin wrapper over the statically-linked FFmpeg (libav*) for the one video
//! operation the recorder needs: trimming a saved replay-buffer file down to the
//! clip we actually want to keep.
//!
//! The trim is a pure remux -- packets are stream-copied, never re-encoded -- so
//! it is fast and lossless. Stream copy can only cut on keyframe boundaries, so
//! the kept clip may start a keyframe or two ahead of the requested point; that
//! is an accepted trade-off for not transcoding (GoldenEye recordings have
//! frequent keyframes, so the slack is small).

use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use ffmpeg_next::ffi::AV_TIME_BASE;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::software::scaling::context::Context as ScalingContext;
use ffmpeg_next::software::scaling::flag::Flags as ScalingFlags;
use ffmpeg_next::util::frame::video::Video;
use ffmpeg_next::{Dictionary, DictionaryRef, Rescale, codec, encoder, format, media, rescale};
use serde::Serialize;

const TAG_CREATED_BY: &str = "fail.acheron.thegoldeneye.created_by";
const TAG_CREATED_BY_VALUE: &str = "the-golden-eye";
const TAG_SCHEMA_VERSION: &str = "fail.acheron.thegoldeneye.schema_version";
const TAG_SCHEMA_VERSION_VALUE: &str = "1";
const TAG_PLUGIN_VERSION: &str = "fail.acheron.thegoldeneye.plugin_version";
const TAG_RUN_TIMESTAMP: &str = "fail.acheron.thegoldeneye.run_timestamp";
const TAG_RUN_TIME: &str = "fail.acheron.thegoldeneye.time";
const TAG_RUN_TIME_SECONDS: &str = "fail.acheron.thegoldeneye.time_seconds";
const TAG_LEVEL: &str = "fail.acheron.thegoldeneye.level";
const TAG_LEVEL_NUMBER: &str = "fail.acheron.thegoldeneye.level_number";
const TAG_DIFFICULTY: &str = "fail.acheron.thegoldeneye.difficulty";
const TAG_STATUS: &str = "fail.acheron.thegoldeneye.status";
const TAG_ROM_LANGUAGE: &str = "fail.acheron.thegoldeneye.rom_language";
const TAG_SOURCE_NAME: &str = "fail.acheron.thegoldeneye.source_name";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipMetadata {
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_seconds: Option<i32>,
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
    pub status: String,
    pub rom_language: String,
    pub source_name: String,
    pub comment: String,
    pub plugin_version: String,
}

impl ClipMetadata {
    fn write_to(&self, metadata: &mut Dictionary) {
        metadata.set(TAG_CREATED_BY, TAG_CREATED_BY_VALUE);
        metadata.set(TAG_SCHEMA_VERSION, TAG_SCHEMA_VERSION_VALUE);
        metadata.set(TAG_PLUGIN_VERSION, &clean_metadata_value(&self.plugin_version));
        metadata.set(TAG_RUN_TIMESTAMP, &clean_metadata_value(&self.timestamp));
        set_optional_metadata(metadata, TAG_RUN_TIME, self.time.as_deref());
        set_optional_metadata(metadata, TAG_RUN_TIME_SECONDS, self.time_seconds.map(|s| s.to_string()).as_deref());
        metadata.set(TAG_LEVEL, &clean_metadata_value(&self.level));
        set_optional_metadata(metadata, TAG_LEVEL_NUMBER, self.level_number.map(|n| n.to_string()).as_deref());
        set_optional_metadata(metadata, TAG_DIFFICULTY, self.difficulty.as_deref());
        metadata.set(TAG_STATUS, &clean_metadata_value(&self.status));
        metadata.set(TAG_ROM_LANGUAGE, &clean_metadata_value(&self.rom_language));
        metadata.set(TAG_SOURCE_NAME, &clean_metadata_value(&self.source_name));
        metadata.set("comment", &clean_metadata_value(&self.comment));
    }

    fn from_dictionary(metadata: &DictionaryRef<'_>) -> Option<Self> {
        let created_by = get_metadata(metadata, TAG_CREATED_BY)?;
        if created_by != TAG_CREATED_BY_VALUE {
            return None;
        }

        let timestamp = get_metadata(metadata, TAG_RUN_TIMESTAMP)?;
        let status = get_metadata(metadata, TAG_STATUS)?;
        let level = get_metadata(metadata, TAG_LEVEL).unwrap_or_else(|| "unknown".to_owned());
        let comment = get_metadata(metadata, "comment").unwrap_or_default();
        let plugin_version = get_metadata(metadata, TAG_PLUGIN_VERSION).unwrap_or_default();
        let time = get_metadata(metadata, TAG_RUN_TIME);
        let time_seconds = get_metadata(metadata, TAG_RUN_TIME_SECONDS).and_then(|value| value.parse::<i32>().ok());
        let level_number = get_metadata(metadata, TAG_LEVEL_NUMBER).and_then(|value| value.parse::<i32>().ok());
        let difficulty = get_metadata(metadata, TAG_DIFFICULTY);
        let rom_language = get_metadata(metadata, TAG_ROM_LANGUAGE).unwrap_or_default();
        let source_name = get_metadata(metadata, TAG_SOURCE_NAME).unwrap_or_default();

        Some(Self {
            timestamp,
            time,
            time_seconds,
            level,
            level_number,
            difficulty,
            status,
            rom_language,
            source_name,
            comment,
            plugin_version,
        })
    }
}

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
    Ok(ClipMetadata::from_dictionary(&ictx.metadata()))
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

/// Decode one video frame and return it as a BMP thumbnail.
pub fn thumbnail_bmp(path: &Path, max_width: u32) -> anyhow::Result<Vec<u8>> {
    init()?;

    let mut ictx = format::input(path).with_context(|| format!("opening {}", path.display()))?;
    let input =
        ictx.streams().best(media::Type::Video).ok_or_else(|| anyhow!("no video stream in {}", path.display()))?;
    let video_stream_index = input.index();

    let context_decoder = codec::context::Context::from_parameters(input.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;
    let (width, height) = thumbnail_dimensions(decoder.width(), decoder.height(), max_width);
    let mut scaler = ScalingContext::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        width,
        height,
        ScalingFlags::BILINEAR,
    )?;

    let mut receive_thumbnail = |decoder: &mut ffmpeg_next::decoder::Video| -> anyhow::Result<Option<Vec<u8>>> {
        let mut decoded = Video::empty();
        if decoder.receive_frame(&mut decoded).is_ok() {
            let mut rgb_frame = Video::empty();
            scaler.run(&decoded, &mut rgb_frame)?;
            return Ok(Some(encode_rgb24_bmp(&rgb_frame)?));
        }
        Ok(None)
    };

    for (stream, packet) in ictx.packets() {
        if stream.index() == video_stream_index {
            decoder.send_packet(&packet)?;
            if let Some(bytes) = receive_thumbnail(&mut decoder)? {
                return Ok(bytes);
            }
        }
    }

    decoder.send_eof()?;
    if let Some(bytes) = receive_thumbnail(&mut decoder)? {
        return Ok(bytes);
    }

    Err(anyhow!("no decodable video frame in {}", path.display()))
}

/// Remux `input` into `output`, keeping only the packets between `start_secs`
/// and `end_secs` (both measured from the start of `input`). Packets are
/// stream-copied, so this neither re-encodes nor decodes; the output container
/// format is inferred from `output`'s extension.
///
/// Timestamps are shifted so the kept clip begins at ~0 in the output (using a
/// single global offset, so audio and video stay in sync). Because the cut is
/// on keyframe boundaries, the real start may be slightly before `start_secs`.
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
        if !is_plugin_metadata_key(key) {
            metadata.set(key, value);
        }
    }
    clip_metadata.write_to(&mut metadata);
    metadata
}

fn is_plugin_metadata_key(key: &str) -> bool {
    [
        TAG_CREATED_BY,
        TAG_SCHEMA_VERSION,
        TAG_PLUGIN_VERSION,
        TAG_RUN_TIMESTAMP,
        TAG_RUN_TIME,
        TAG_RUN_TIME_SECONDS,
        TAG_LEVEL,
        TAG_LEVEL_NUMBER,
        TAG_DIFFICULTY,
        TAG_STATUS,
        TAG_ROM_LANGUAGE,
        TAG_SOURCE_NAME,
        "comment",
    ]
    .iter()
    .any(|candidate| key.eq_ignore_ascii_case(candidate))
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

fn set_optional_metadata(metadata: &mut Dictionary, key: &str, value: Option<&str>) {
    if let Some(value) = value
        && !value.is_empty()
    {
        metadata.set(key, &clean_metadata_value(value));
    }
}

fn get_metadata(metadata: &DictionaryRef<'_>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .or_else(|| metadata.iter().find(|(candidate, _)| candidate.eq_ignore_ascii_case(key)).map(|(_, value)| value))
        .map(str::to_owned)
        .filter(|value| !value.is_empty())
}

fn clean_metadata_value(value: &str) -> String {
    value.chars().filter(|&c| c != '\0').collect()
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

fn thumbnail_dimensions(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (1, 1);
    }
    let out_width = width.min(max_width.max(1));
    let out_height = ((height as u64 * out_width as u64) / width as u64).clamp(1, u32::MAX as u64) as u32;
    (out_width, out_height)
}

fn encode_rgb24_bmp(frame: &Video) -> std::io::Result<Vec<u8>> {
    let width = frame.width();
    let height = frame.height();
    let stride = frame.stride(0);
    let data = frame.data(0);

    let mut image = bmp::Image::new(width, height);
    for y in 0..height {
        let row = &data[(y as usize * stride)..];
        for x in 0..width {
            let i = x as usize * 3;
            image.set_pixel(x, y, bmp::Pixel::new(row[i], row[i + 1], row[i + 2]));
        }
    }

    let mut out = Cursor::new(Vec::new());
    image.to_writer(&mut out)?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_clip() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/clips/sample_clip.mov")
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

    #[test]
    fn trims_with_metadata_and_reads_it_back() {
        let input = sample_clip();
        let full = duration_secs(&input).expect("probe duration");
        let out = std::env::temp_dir().join(format!("ge_ffmpeg_metadata_test_{}.mov", std::process::id()));
        let _ = std::fs::remove_file(&out);

        let metadata = ClipMetadata {
            timestamp: "2026-01-02T03:04:05Z".to_owned(),
            time: Some("02:03".to_owned()),
            time_seconds: Some(123),
            level: "Surface 2".to_owned(),
            level_number: Some(8),
            difficulty: Some("00 Agent".to_owned()),
            status: "complete".to_owned(),
            rom_language: "en".to_owned(),
            source_name: "N64 Capture".to_owned(),
            comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
            plugin_version: "0.0.0".to_owned(),
        };

        trim_with_metadata(&input, &out, 1.0, (full - 1.0).max(2.0), Some(&metadata)).expect("trim with metadata");

        let got = read_clip_metadata(&out).expect("read metadata").expect("plugin metadata");
        assert_eq!(got, metadata);

        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn rewrites_metadata_in_place_and_drops_old_optional_tags() {
        let input = sample_clip();
        let full = duration_secs(&input).expect("probe duration");
        let out = std::env::temp_dir().join(format!("ge_ffmpeg_metadata_rewrite_test_{}.mov", std::process::id()));
        let _ = std::fs::remove_file(&out);

        let original = ClipMetadata {
            timestamp: "2026-01-02T03:04:05Z".to_owned(),
            time: Some("02:03".to_owned()),
            time_seconds: Some(123),
            level: "Surface 2".to_owned(),
            level_number: Some(8),
            difficulty: Some("00 Agent".to_owned()),
            status: "complete".to_owned(),
            rom_language: "en".to_owned(),
            source_name: "N64 Capture".to_owned(),
            comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
            plugin_version: "0.0.0".to_owned(),
        };
        trim_with_metadata(&input, &out, 1.0, (full - 1.0).max(2.0), Some(&original)).expect("trim with metadata");

        let updated = ClipMetadata {
            timestamp: "2026-01-03T03:04:05Z".to_owned(),
            time: None,
            time_seconds: None,
            level: "Dam".to_owned(),
            level_number: Some(1),
            difficulty: None,
            status: "failed".to_owned(),
            rom_language: "jp".to_owned(),
            source_name: "N64 Capture".to_owned(),
            comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
            plugin_version: "0.0.0".to_owned(),
        };
        rewrite_metadata_in_place(&out, &updated).expect("rewrite metadata");

        let got = read_clip_metadata(&out).expect("read metadata").expect("plugin metadata");
        assert_eq!(got, updated);

        let _ = std::fs::remove_file(&out);
    }
}
