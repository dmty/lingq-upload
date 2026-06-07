//! Soft-delete subsystem for projects.
//!
//! Trashing renames `<root>/projects/<slug>` to `<root>/projects/.trash/<slug>-<unix_ts>/`.
//! Restore reverses the move (failing if a live project already occupies the slot).
//! Purge runs `remove_dir_all`. See AD-024.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::index::LibraryError;
use crate::core::identity::ProjectId;
use crate::core::project::Project;
use crate::core::store::safe_path_segment;

pub const TRASH_DIRNAME: &str = ".trash";

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct TrashEntry {
    pub trash_id: String,
    pub project_id: ProjectId,
    pub title: String,
    pub language: String,
    pub trashed_at: DateTime<Utc>,
}

fn projects_dir(root: &Path) -> PathBuf {
    root.join("projects")
}

fn trash_dir(root: &Path) -> PathBuf {
    projects_dir(root).join(TRASH_DIRNAME)
}

fn now_unix() -> Result<i64, LibraryError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|_| LibraryError::Io("system clock before unix epoch".into()))
}

fn dir_modified(dir: &Path) -> Result<DateTime<Utc>, LibraryError> {
    let meta = fs::metadata(dir).map_err(|e| LibraryError::Io(e.to_string()))?;
    let modified = meta
        .modified()
        .map_err(|e| LibraryError::Io(e.to_string()))?;
    Ok(DateTime::<Utc>::from(modified))
}

pub fn trash_project(
    projects_root: &Path,
    project_id: &ProjectId,
) -> Result<TrashEntry, LibraryError> {
    let slug = safe_path_segment(&project_id.join_key());
    let src = projects_dir(projects_root).join(&slug);
    if !src.is_dir() {
        return Err(LibraryError::NotFound(format!(
            "project dir missing: {}",
            src.display()
        )));
    }
    let trash_root = trash_dir(projects_root);
    fs::create_dir_all(&trash_root).map_err(|e| LibraryError::Io(e.to_string()))?;

    // Stamp identity from the still-live project.json before the move so a
    // post-rename JSON failure doesn't leave a project in trash that the UI
    // can't describe.
    let project = read_project(&src)?;

    let ts = now_unix()?;
    let mut trash_id = format!("{slug}-{ts}");
    let mut dst = trash_root.join(&trash_id);
    let mut n = 1;
    while dst.exists() {
        trash_id = format!("{slug}-{ts}-{n}");
        dst = trash_root.join(&trash_id);
        n += 1;
    }
    fs::rename(&src, &dst).map_err(|e| LibraryError::Io(e.to_string()))?;

    let trashed_at = dir_modified(&dst).unwrap_or_else(|_| Utc::now());
    Ok(TrashEntry {
        trash_id,
        project_id: project.id,
        title: project.settings.collection_title,
        language: project.settings.language,
        trashed_at,
    })
}

pub fn list_trash(projects_root: &Path) -> Result<Vec<TrashEntry>, LibraryError> {
    let trash_root = trash_dir(projects_root);
    if !trash_root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = fs::read_dir(&trash_root).map_err(|e| LibraryError::Io(e.to_string()))?;
    for ent in entries {
        let ent = ent.map_err(|e| LibraryError::Io(e.to_string()))?;
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let Some(trash_id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let project = match read_project(&path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("library/trash: skipping {trash_id}: {e}");
                continue;
            }
        };
        let trashed_at = match dir_modified(&path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("library/trash: mtime unavailable for {trash_id}: {e}");
                continue;
            }
        };
        out.push(TrashEntry {
            trash_id,
            project_id: project.id,
            title: project.settings.collection_title,
            language: project.settings.language,
            trashed_at,
        });
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.trashed_at));
    Ok(out)
}

pub fn restore_project(projects_root: &Path, trash_id: &str) -> Result<(), LibraryError> {
    let src = trash_dir(projects_root).join(trash_id);
    if !src.is_dir() {
        return Err(LibraryError::NotFound(format!(
            "trash entry not found: {trash_id}"
        )));
    }
    let project = read_project(&src)?;
    let slug = safe_path_segment(&project.id.join_key());
    let dst = projects_dir(projects_root).join(&slug);
    if dst.exists() {
        return Err(LibraryError::Conflict(format!(
            "project already exists at slot: {slug}"
        )));
    }
    fs::rename(&src, &dst).map_err(|e| LibraryError::Io(e.to_string()))?;
    Ok(())
}

pub fn purge_project(projects_root: &Path, trash_id: &str) -> Result<(), LibraryError> {
    let dir = trash_dir(projects_root).join(trash_id);
    if !dir.is_dir() {
        return Err(LibraryError::NotFound(format!(
            "trash entry not found: {trash_id}"
        )));
    }
    fs::remove_dir_all(&dir).map_err(|e| LibraryError::Io(e.to_string()))?;
    Ok(())
}

fn read_project(dir: &Path) -> Result<Project, LibraryError> {
    let pj = dir.join("project.json");
    let bytes = fs::read(&pj).map_err(|e| LibraryError::Io(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| LibraryError::Io(e.to_string()))
}
