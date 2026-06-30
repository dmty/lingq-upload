use crate::core::audio::AbsorbPolicy;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;

use crate::core::epub::cover::extract_to_dir;
use crate::core::identity::ProjectId;
use crate::core::library::{candidate_to_id, rebuild_from_store, write_atomic, INDEX_FILENAME};
use crate::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::ingest::{Candidate, TextSource};

/// Upper bound on copy-name allocation attempts. A book with 100 colliding
/// titles in the store is almost certainly a bug, not a legitimate user state.
const MAX_COPY_ATTEMPTS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CreateProjectResult {
    Created {
        id: ProjectId,
    },
    Conflict {
        existing: ProjectId,
        conflict_title: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    Replace,
    Skip,
    NewProject,
}

fn build_project(candidate: &Candidate, language: String, collection_title: String) -> Project {
    let id = candidate_to_id(candidate);
    Project {
        schema_version: SCHEMA_V1,
        id,
        sources: ProjectSources {
            text: candidate.text_source.clone(),
            audio: candidate.audio_source.clone(),
            chapter_manifest: candidate.chapter_manifest.clone(),
        },
        settings: ProjectSettings {
            language,
            collection_title,
            level: 1,
            tags: vec![],
        },
        receipts: vec![],
        queue_cursor: 0,
        completed_lesson_ids: vec![],
        matcher_decision: None,
        cover_path: candidate.cover_path.clone(),
        authors: candidate.authors.clone(),
        series: candidate.series.clone(),
        lingq_collection_id: None,
        last_activity_at: None,
        stage: Default::default(),
        last_transition_at: None,
        skipped_chapters: vec![],
        absorb_policy: AbsorbPolicy::default(),
        mapping: None,
        confirmed_at: None,
        cover_use: true,
        cover_uploaded_to_lingq: false,
        cover_source_href: None,
    }
}

/// If the project has no cover and its text source is an EPUB, attempt to
/// extract a cover image into the project's store directory. Soft-fails:
/// extraction errors are logged and the project proceeds without a cover.
fn try_extract_epub_cover(project: &mut Project, store: &dyn ProjectStore) {
    if project.cover_path.is_some() {
        return;
    }
    let TextSource::Epub(epub_path) = &project.sources.text else {
        return;
    };
    let Some(dest_dir) = store.project_dir(&project.id) else {
        tracing::debug!(id = %project.id.join_key(), "store has no project_dir; skipping epub cover extraction");
        return;
    };
    let epub_path = epub_path.clone();
    match extract_to_dir(&epub_path, &dest_dir) {
        Ok(Some(cov)) => {
            tracing::debug!(path = %cov.path.display(), "extracted epub cover");
            project.cover_source_href = cov.source_spine_href;
            project.cover_path = Some(cov.path);
        }
        Ok(None) => {
            tracing::debug!(epub = %epub_path.display(), "no cover found in epub");
        }
        Err(e) => {
            tracing::warn!(epub = %epub_path.display(), error = %e, "epub cover extraction failed; continuing without cover");
        }
    }
}

/// Build, optionally enrich with an extracted EPUB cover, and persist a
/// project. Returns the persisted `Project`. Exposed for integration tests.
pub fn add_project_impl(
    store: &dyn ProjectStore,
    candidate: &Candidate,
    language: String,
    collection_title: String,
) -> Result<Project, AppError> {
    let mut project = build_project(candidate, language, collection_title);
    try_extract_epub_cover(&mut project, store);
    store
        .put(&project)
        .map_err(|e| AppError::Other(format!("store.put: {e}")))?;
    Ok(project)
}

fn rebuild_library_index(app: &tauri::AppHandle, store: &dyn ProjectStore) -> Result<(), AppError> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    let idx =
        rebuild_from_store(store).map_err(|e| AppError::Other(format!("library rebuild: {e}")))?;
    write_atomic(&idx, &root.join(INDEX_FILENAME))
        .map_err(|e| AppError::Other(format!("library write: {e}")))?;
    Ok(())
}

/// Persist a project and refresh the on-disk library index in one shot.
/// Centralises the put + reindex + atomic-write triple so each conflict
/// resolution branch can't drift from the others.
fn persist_and_reindex(
    app: &tauri::AppHandle,
    store: &Arc<dyn ProjectStore>,
    project: &Project,
) -> Result<(), AppError> {
    store
        .put(project)
        .map_err(|e| AppError::Other(format!("store.put: {e}")))?;
    rebuild_library_index(app, store.as_ref())
}

/// Extract cover (if applicable), persist, reindex. Returns the project id.
fn enrich_and_persist(
    app: &tauri::AppHandle,
    store: &Arc<dyn ProjectStore>,
    project: &mut Project,
) -> Result<ProjectId, AppError> {
    let id = project.id.clone();
    try_extract_epub_cover(project, store.as_ref());
    persist_and_reindex(app, store, project)?;
    Ok(id)
}

/// Persist a Candidate as a Project. Returns `Created` with the stable
/// `ProjectId`, or `Conflict { existing, conflict_title }` if a project
/// with the derived id already exists. On conflict no write occurs — the
/// caller resolves via `cmd_create_project_with_resolution`.
#[tauri::command]
#[specta::specta]
pub async fn cmd_create_project(
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    candidate: Candidate,
    language: String,
    collection_title: String,
) -> Result<CreateProjectResult, AppError> {
    let mut project = build_project(&candidate, language, collection_title);

    if let Some(existing) = store
        .get(&project.id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
    {
        return Ok(CreateProjectResult::Conflict {
            existing: existing.id.clone(),
            conflict_title: existing.settings.collection_title,
        });
    }

    let id = enrich_and_persist(&app, store.inner(), &mut project)?;

    Ok(CreateProjectResult::Created { id })
}

/// Resolve a create-project conflict by user choice.
///
/// - `Replace` overwrites the existing project at the conflict id.
/// - `Skip` returns the conflict id directly without re-reading the store
///   (the conflict was already detected in `cmd_create_project`; a second
///   `store.get` would race against a delete).
/// - `NewProject` mutates the candidate's *title* and re-derives the id
///   until it lands on an unused content hash. The id is hashed from
///   `candidate.title + authors[0]`, so mutating `collection_title` alone
///   keeps the hash constant and loops forever — append `" (copy)"` to
///   `candidate.title` instead, capped at `MAX_COPY_ATTEMPTS`.
#[tauri::command]
#[specta::specta]
pub async fn cmd_create_project_with_resolution(
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    candidate: Candidate,
    language: String,
    collection_title: String,
    resolution: ConflictResolution,
) -> Result<ProjectId, AppError> {
    match resolution {
        ConflictResolution::Replace => {
            let mut project = build_project(&candidate, language, collection_title);
            enrich_and_persist(&app, store.inner(), &mut project)
        }
        ConflictResolution::Skip => Ok(candidate_to_id(&candidate)),
        ConflictResolution::NewProject => {
            let mut copy = candidate.clone();
            for _ in 0..MAX_COPY_ATTEMPTS {
                copy.title.push_str(" (copy)");
                let mut project = build_project(&copy, language.clone(), copy.title.clone());
                let exists = store
                    .get(&project.id)
                    .map_err(|e| AppError::Other(format!("store.get: {e}")))?
                    .is_some();
                if !exists {
                    return enrich_and_persist(&app, store.inner(), &mut project);
                }
            }
            Err(AppError::Other("could not allocate copy name".into()))
        }
    }
}
