use ge_settings::DEFAULT_CLIP_FILENAME_TEMPLATE;

use super::*;
use crate::run_monitoring::clip_output::{append_extension, sanitize_relative_clip_path};
use crate::template_tokens::RunTemplateTokens;

pub(super) fn updated_clip_path(settings: &AppSettings, run: &RunRecord, metadata: &ClipMetadata) -> Option<PathBuf> {
    let clip = run.clip.as_ref()?;
    let root = configured_run_directories(settings)
        .into_iter()
        .filter_map(|dir| fs::canonicalize(dir.path).ok())
        .filter(|dir| clip.path.starts_with(dir))
        .max_by_key(|dir| dir.components().count())
        .unwrap_or_else(|| clip.path.parent().unwrap_or(Path::new(".")).to_path_buf());
    let template = if settings.clip_filename_template.trim().is_empty() {
        DEFAULT_CLIP_FILENAME_TEMPLATE
    } else {
        &settings.clip_filename_template
    };
    let ext = clip.path.extension().and_then(|value| value.to_str()).unwrap_or("");
    let current = clip.path.strip_prefix(&root).ok()?.with_extension("");
    let current = current.to_str()?;
    let pattern = sanitize_relative_clip_path(
        &RunTemplateTokens::from_clip_metadata("GE_REPLAY_NAME", &run.metadata).render(template),
    );
    let pattern = pattern.as_deref().and_then(Path::to_str).unwrap_or("");
    let stem = replay_stem(current, pattern)
        .unwrap_or_else(|| clip.path.file_stem().and_then(|value| value.to_str()).unwrap_or("replay"));
    let old = RunTemplateTokens::from_clip_metadata(stem, &run.metadata).render(template);
    let new = RunTemplateTokens::from_clip_metadata(stem, metadata).render(template);
    if old == new {
        return None;
    }
    let relative = sanitize_relative_clip_path(&new).or_else(|| {
        sanitize_relative_clip_path(
            &RunTemplateTokens::from_clip_metadata(stem, metadata).render(DEFAULT_CLIP_FILENAME_TEMPLATE),
        )
    })?;
    Some(root.join(append_extension(relative, ext)))
}

fn replay_stem<'a>(current: &'a str, pattern: &str) -> Option<&'a str> {
    let (prefix, suffix) = pattern.split_once("GE_REPLAY_NAME")?;
    let extract = |path: &'a str| path.strip_prefix(prefix)?.strip_suffix(suffix);
    extract(current).or_else(|| {
        let (path, number) = current.strip_suffix(')')?.rsplit_once(" (")?;
        (number.parse::<usize>().ok()? >= 2).then(|| extract(path)).flatten()
    })
}
