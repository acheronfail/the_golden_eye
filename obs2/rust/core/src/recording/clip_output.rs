fn clip_metadata(
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<&LevelMatch>,
    source_name: &str,
    game_language: &str,
) -> ClipMetadata {
    let level_info = stats.and_then(|m| ge::level_info(m.mission, m.part));
    let time_seconds = stats.and_then(|m| m.times.map(|times| times.time.max(0)));

    ClipMetadata {
        run_id: String::new(),
        timestamp: format_iso_utc(completed_at),
        time: time_seconds.map(format_time),
        time_seconds,
        level: level_info.map(|info| info.name.to_owned()).unwrap_or_else(|| "unknown".to_owned()),
        level_number: level_info.map(|info| info.number),
        difficulty: stats.and_then(|m| ge::difficulty_name(m.difficulty)).map(str::to_owned),
        status,
        was_personal_best: false,
        game_language: game_language.to_owned(),
        rom_version: None,
        source_name: source_name.to_owned(),
        comment: format!("Created by The Golden Eye OBS plugin v{}", crate::PLUGIN_VERSION),
        plugin_version: crate::PLUGIN_VERSION.to_owned(),
        retention_state: "pending".to_owned(),
        retention_reason: None,
    }
}

fn ensure_output_directory(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating output directory {}", dir.display()))?;

    let metadata = std::fs::metadata(dir).with_context(|| format!("checking output directory {}", dir.display()))?;
    if !metadata.is_dir() {
        anyhow::bail!("output path {} exists but is not a directory", dir.display());
    }

    Ok(())
}

fn output_dir(input: &Path, policy: &ClipOutputPolicy) -> PathBuf {
    if let Some(path) = &policy.output_directory {
        return path.clone();
    }
    input.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
}

fn configured_dir(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(expand_home(trimmed))
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~"
        && let Some(home) = crate::config::home_dir()
    {
        return home;
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = crate::config::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn unique_output_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("clip");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    for i in 2.. {
        let file_name = if ext.is_empty() { format!("{stem} ({i})") } else { format!("{stem} ({i}).{ext}") };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unbounded filename suffix search should always return")
}

/// Build an output path from the configured template and matched level info.
/// Collisions are handled by [`unique_output_path`], so terse templates remain
/// safe even when multiple runs render to the same relative path.
fn clip_relative_path(
    stem: &str,
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<&LevelMatch>,
    template: &str,
) -> PathBuf {
    let rendered = render_clip_template(template, stem, status, completed_at, stats);
    if let Some(path) = sanitize_relative_clip_path(&rendered) {
        path
    } else {
        sanitize_relative_clip_path(&render_clip_template(
            DEFAULT_CLIP_FILENAME_TEMPLATE,
            stem,
            status,
            completed_at,
            stats,
        ))
        .unwrap_or_else(|| PathBuf::from("clip"))
    }
}

fn render_clip_template(
    template: &str,
    stem: &str,
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<&LevelMatch>,
) -> String {
    RunTemplateTokens::from_match(stem, status.as_str(), completed_at, stats).render(template)
}

#[cfg_attr(test, allow(dead_code))]
fn append_extension(mut path: PathBuf, ext: &str) -> PathBuf {
    if ext.is_empty() {
        return path;
    }

    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("clip");
    path.set_file_name(format!("{file_name}.{ext}"));
    path
}

fn sanitize_relative_clip_path(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains(wrong_platform_separator()) {
        return None;
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return None;
    }

    let mut sanitized = PathBuf::new();
    for component in trimmed.split(std::path::MAIN_SEPARATOR) {
        let component = sanitize_path_component(component);
        if component.is_empty() || component == "." || component == ".." {
            return None;
        }
        sanitized.push(component);
    }

    Some(sanitized)
}

fn wrong_platform_separator() -> char {
    if std::path::MAIN_SEPARATOR == '/' { '\\' } else { '/' }
}

fn sanitize_path_component(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect::<String>()
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_owned()
}
