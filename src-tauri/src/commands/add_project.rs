use std::sync::Arc;

use tauri::Manager;

use crate::core::identity::ProjectId;
use crate::core::library::{candidate_to_id, rebuild_from_store, write_atomic, INDEX_FILENAME};
use crate::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::ingest::Candidate;

/// Persist a Candidate as a Project. Returns the stable `ProjectId`.
/// If a matching project already exists the existing project is replaced.
#[tauri::command]
#[specta::specta]
pub async fn cmd_create_project(
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    candidate: Candidate,
    language: String,
    collection_title: String,
) -> Result<ProjectId, AppError> {
    let id = candidate_to_id(&candidate);
    let project = Project {
        schema_version: SCHEMA_V1,
        id: id.clone(),
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
    };
    store
        .put(&project)
        .map_err(|e| AppError::Other(format!("store.put: {e}")))?;

    // Keep `library.index.json` in lockstep so the next `cmd_library_list`
    // sees the new project even if it short-circuits on the cache.
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    let idx = rebuild_from_store(store.inner().as_ref())
        .map_err(|e| AppError::Other(format!("library rebuild: {e}")))?;
    write_atomic(&idx, &root.join(INDEX_FILENAME))
        .map_err(|e| AppError::Other(format!("library write: {e}")))?;

    Ok(id)
}
