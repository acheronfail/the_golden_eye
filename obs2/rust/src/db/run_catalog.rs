use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::Context;
use rusqlite::Connection;

use super::clips::{self, ClipValidation};
use super::meta;
use crate::models::clip_metadata::ClipMetadata;
use crate::youtube::{UploadHistoryEntry, YoutubeMetadata};

const DB_FILE_NAME: &str = "runs.sqlite";

#[derive(Debug, Clone)]
pub struct RunCatalogRoot {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct IndexedRunClip {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: Option<SystemTime>,
    pub duration_secs: Option<f64>,
    pub metadata: ClipMetadata,
}

#[derive(Debug, Clone)]
pub struct RunCatalogSave {
    pub path: PathBuf,
    pub duration_secs: Option<f64>,
    pub metadata: ClipMetadata,
}

/// SQLite-backed index of local tagged clips plus YouTube upload history. The
/// clip container tags are the source of truth; this catalog is a fast cache.
pub struct RunCatalog {
    conn: Mutex<Connection>,
}

impl RunCatalog {
    pub fn path_for_settings(settings_path: &Path) -> PathBuf {
        settings_path.parent().unwrap_or_else(|| Path::new(".")).join(DB_FILE_NAME)
    }

    pub fn exists_for_settings(settings_path: &Path) -> bool {
        Self::path_for_settings(settings_path).exists()
    }

    pub fn open_for_settings(settings_path: &Path) -> anyhow::Result<Self> {
        Self::open(Self::path_for_settings(settings_path))
    }

    pub fn open(db_path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            tracing::info!(path = %parent.display(), "creating run catalog directory");
            fs::create_dir_all(parent)
                .with_context(|| format!("creating run catalog directory {}", parent.display()))?;
        }
        tracing::info!(path = %db_path.display(), "opening SQLite run catalog");
        let conn = Connection::open(&db_path).with_context(|| format!("opening run catalog {}", db_path.display()))?;
        initialise_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn list(&self, roots: &[RunCatalogRoot]) -> anyhow::Result<Vec<IndexedRunClip>> {
        let clips = {
            let conn = self.lock();
            clips::list(&conn)?
        };

        let mut valid = Vec::new();
        for clip in clips {
            if !clips::is_under_roots(&clip.path, roots) {
                continue;
            }
            match clips::validate_clip(&clip) {
                ClipValidation::Unchanged => valid.push(clip),
                ClipValidation::Missing => {
                    self.remove_path(&clip.path)?;
                }
                ClipValidation::Changed => match clips::read_from_disk(&clip.path) {
                    Ok(Some(updated)) => {
                        self.upsert_clip(&updated)?;
                        valid.push(updated);
                    }
                    Ok(None) => self.remove_path(&clip.path)?,
                    Err(err) => {
                        tracing::debug!(path = %clip.path.display(), "removing unreadable indexed run clip: {err:#}");
                        self.remove_path(&clip.path)?;
                    }
                },
            }
        }
        valid.sort_by(|a, b| {
            b.metadata
                .timestamp
                .cmp(&a.metadata.timestamp)
                .then_with(|| b.modified.cmp(&a.modified))
                .then_with(|| b.path.cmp(&a.path))
        });
        Ok(valid)
    }

    pub fn resync(&self, roots: &[RunCatalogRoot]) -> anyhow::Result<()> {
        tracing::info!(roots = roots.len(), "resyncing run catalog from filesystem");
        let mut discovered = HashSet::new();
        for root in roots {
            tracing::info!(root = %root.path.display(), "scanning run catalog root");
            clips::ensure_directory(&root.path)?;
            for path in clips::video_files_in_directory_recursive(&root.path)? {
                let path = clips::catalog_path(&path);
                discovered.insert(path.clone());
                match clips::read_from_disk(&path) {
                    Ok(Some(clip)) => self.upsert_clip(&clip)?,
                    Ok(None) => self.remove_path(&path)?,
                    Err(err) => {
                        tracing::debug!(path = %path.display(), "removing unreadable run catalog resync candidate: {err:#}");
                        self.remove_path(&path)?;
                    }
                }
            }
        }

        for path in self.indexed_paths()? {
            if !clips::is_under_roots(&path, roots) || !discovered.contains(&path) {
                self.remove_path(&path)?;
            }
        }
        Ok(())
    }

    pub fn record_saved_clip(&self, save: RunCatalogSave) -> anyhow::Result<IndexedRunClip> {
        let clip = clips::record_saved(save)?;
        tracing::info!(path = %clip.path.display(), status = clip.metadata.status.as_str(), "recording saved clip in run catalog");
        self.upsert_clip(&clip)?;
        Ok(clip)
    }

    pub fn refresh_clip(&self, path: &Path) -> anyhow::Result<Option<IndexedRunClip>> {
        let path = clips::catalog_path(path);
        match clips::read_from_disk(&path)? {
            Some(clip) => {
                tracing::info!(path = %clip.path.display(), status = clip.metadata.status.as_str(), "refreshing clip in run catalog");
                self.upsert_clip(&clip)?;
                Ok(Some(clip))
            }
            None => {
                self.remove_path(&path)?;
                Ok(None)
            }
        }
    }

    pub fn rename_path(&self, from: &Path, to: &Path) -> anyhow::Result<()> {
        let from = clips::catalog_path(from);
        let to = clips::catalog_path(to);
        tracing::info!(from = %from.display(), to = %to.display(), "renaming clip in run catalog");
        let conn = self.lock();
        clips::rename_path(&conn, &from, &to)
    }

    pub fn remove_path(&self, path: &Path) -> anyhow::Result<()> {
        let path = clips::catalog_path(path);
        tracing::info!(path = %path.display(), "removing clip from run catalog");
        let conn = self.lock();
        clips::remove_path(&conn, &path)
    }

    pub fn pending_failed_reviews(&self) -> anyhow::Result<Vec<IndexedRunClip>> {
        let conn = self.lock();
        clips::pending_failed_reviews(&conn)
    }

    pub fn failed_review_is_pending(&self, path: &Path) -> anyhow::Result<bool> {
        let conn = self.lock();
        clips::failed_review_is_pending(&conn, path)
    }

    pub fn keep_failed_reviews(&self, paths: &[PathBuf]) -> anyhow::Result<()> {
        let mut conn = self.lock();
        clips::keep_failed_reviews(&mut conn, paths)
    }

    pub fn youtube_history(&self) -> anyhow::Result<Vec<UploadHistoryEntry>> {
        let conn = self.lock();
        clips::youtube_history(&conn)
    }

    pub fn set_youtube_history(&self, path: &Path, youtube: &YoutubeMetadata) -> anyhow::Result<()> {
        let conn = self.lock();
        clips::set_youtube_history(&conn, path, youtube)
    }

    pub fn forget_youtube_history_for_display_path(&self, display_path: &str) -> anyhow::Result<usize> {
        let conn = self.lock();
        clips::clear_youtube_history(&conn, Path::new(display_path))
    }

    fn upsert_clip(&self, clip: &IndexedRunClip) -> anyhow::Result<()> {
        let mut conn = self.lock();
        clips::upsert(&mut conn, clip)
    }

    fn indexed_paths(&self) -> anyhow::Result<Vec<PathBuf>> {
        let conn = self.lock();
        clips::indexed_paths(&conn)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn initialise_schema(conn: &Connection) -> anyhow::Result<()> {
    if meta::needs_reset(conn)? {
        clips::drop_tables(conn)?;
        meta::drop_tables(conn)?;
    }
    meta::initialise(conn)?;
    clips::initialise(conn)?;
    Ok(())
}

#[cfg(test)]
#[path = "run_catalog_test.rs"]
mod run_catalog_test;
