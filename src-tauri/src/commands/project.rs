use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::audio::AbsorbPolicy;
use crate::core::epub::{parse_epub, Chapter, ChapterId, ChapterKind};
use crate::core::identity::ProjectId;
use crate::core::project::Project;
use crate::core::store::{ProjectStore, StoreError};
use crate::core::text::read_text_for_upload;
use crate::error::AppError;
use crate::ingest::TextSource;

/// Picker-facing projection of [`Chapter`] without the body. Picker rows only
/// need identity + label + kind; shipping the body over IPC would cost tens
/// of MB on book-length inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ChapterMeta {
    pub id: ChapterId,
    pub order: usize,
    pub title: String,
    pub kind: ChapterKind,
}

impl From<Chapter> for ChapterMeta {
    fn from(c: Chapter) -> Self {
        Self {
            id: c.id,
            order: c.order,
            title: c.title,
            kind: c.kind,
        }
    }
}

/// Load a persisted project by its `join_key`.
///
/// The frontend reaches Match and Run routes with a stringified key in the URL
/// (e.g. `asin:B0...`, `isbn13:978...`, `uuid:...`, `ch:<hex>`). This command
/// resolves that key back to the full [`Project`] so the UI can render
/// receipts, settings, and rebuild a ProjectId for typed downstream commands
/// (notably `cmd_matcher_resolve`).
#[tauri::command]
#[specta::specta]
pub async fn cmd_project_load(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    key: String,
) -> Result<Project, AppError> {
    let summaries = store
        .list()
        .map_err(|e| AppError::Other(format!("store.list: {e}")))?;
    let summary = summaries
        .into_iter()
        .find(|s| s.id.join_key() == key)
        .ok_or_else(|| AppError::Other(format!("project not found: {key}")))?;
    let project = store
        .get(&summary.id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other(format!("project not found: {key}")))?;
    Ok(project)
}

/// Replace the project's skipped-chapter set wholesale.
///
/// The picker UI debounces user edits and flushes the resulting selection
/// here. `ProjectStore::set_selection` is atomic per AD-022.
#[tauri::command]
#[specta::specta]
pub async fn cmd_set_selection(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    skipped_ids: Vec<ChapterId>,
) -> Result<(), AppError> {
    store
        .set_selection(&project_id, &skipped_ids)
        .map_err(|e| match e {
            StoreError::NotFound { key } => AppError::Other(format!("project not found: {key}")),
            other => AppError::Other(format!("store.set_selection: {other}")),
        })?;
    Ok(())
}

/// List the parsed chapters of a project's text source so the picker can
/// render rows. Re-parses on each call — the picker is short-lived UI so
/// the cost is acceptable; a future iteration may cache.
#[tauri::command]
#[specta::specta]
pub async fn cmd_project_chapters(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
) -> Result<Vec<ChapterMeta>, AppError> {
    let project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    match &project.sources.text {
        TextSource::Epub(path) => {
            let chapters =
                parse_epub(path).map_err(|e| AppError::Other(format!("parse_epub: {e}")))?;
            Ok(chapters.into_iter().map(ChapterMeta::from).collect())
        }
        TextSource::LooseFiles { paths } => Ok(paths
            .iter()
            .enumerate()
            .map(|(i, p)| ChapterMeta {
                id: ChapterId::from_order(i),
                order: i,
                title: p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("Chapter {}", i + 1)),
                kind: ChapterKind::default(),
            })
            .collect()),
        TextSource::Missing => Ok(Vec::new()),
    }
}

/// Return one chapter's body on demand. The list projection (`ChapterMeta`) is
/// body-less; callers fetch full text only when they need to inspect it.
#[tauri::command]
#[specta::specta]
pub async fn cmd_chapter_text(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    chapter_id: ChapterId,
) -> Result<String, AppError> {
    let project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    match &project.sources.text {
        TextSource::Epub(path) => {
            let chapters =
                parse_epub(path).map_err(|e| AppError::Other(format!("parse_epub: {e}")))?;
            chapters
                .into_iter()
                .find(|c| c.id == chapter_id)
                .map(|c| c.body)
                .ok_or_else(|| AppError::Other(format!("chapter '{chapter_id}' not found")))
        }
        TextSource::LooseFiles { paths } => {
            let idx = chapter_id
                .order_index()
                .ok_or_else(|| {
                    AppError::Other(format!(
                        "chapter id '{chapter_id}' is not order-indexed"
                    ))
                })?;
            let path = paths
                .get(idx)
                .ok_or_else(|| AppError::Other(format!("chapter index {idx} out of range")))?;
            read_text_for_upload(path).map_err(|e| AppError::Other(format!("read: {e}")))
        }
        TextSource::Missing => Err(AppError::Other("project has no text source".into())),
    }
}

/// Persist the chapter-divider absorb policy for a project.
#[tauri::command]
#[specta::specta]
pub async fn cmd_set_absorb_policy(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    policy: AbsorbPolicy,
) -> Result<(), AppError> {
    store
        .update(&project_id, &mut |p| p.absorb_policy = policy)
        .map_err(|e| match e {
            StoreError::NotFound { key } => AppError::Other(format!("project not found: {key}")),
            other => AppError::Other(format!("store.update: {other}")),
        })?;
    Ok(())
}

/// Persist a user-chosen cover image path for a project. Display only — the
/// cover is never uploaded to LingQ. The frontend renders it via
/// `convertFileSrc(cover_path)`.
#[tauri::command]
#[specta::specta]
pub async fn cmd_set_cover(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    cover_path: String,
) -> Result<(), AppError> {
    store
        .update(&project_id, &mut |p| {
            p.cover_path = Some(std::path::PathBuf::from(&cover_path))
        })
        .map_err(|e| match e {
            StoreError::NotFound { key } => AppError::Other(format!("project not found: {key}")),
            other => AppError::Other(format!("store.update: {other}")),
        })?;
    Ok(())
}

