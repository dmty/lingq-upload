use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;

use crate::core::identity::ProjectId;
use crate::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use crate::core::store::{JsonProjectStore, ProjectStore};
use crate::error::AppError;
use crate::ingest::Candidate;

/// Persist a Candidate as a Project. Returns the stable `ProjectId`.
/// If a matching project already exists the existing project is replaced.
#[tauri::command]
#[specta::specta]
pub async fn cmd_create_project(
    app: tauri::AppHandle,
    candidate: Candidate,
    language: String,
    collection_title: String,
) -> Result<ProjectId, AppError> {
    let root: PathBuf = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    let store: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(root));

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
    Ok(id)
}

fn candidate_to_id(c: &Candidate) -> ProjectId {
    let author = c.authors.first().map(|s| s.as_str()).unwrap_or("");
    let mut id = ProjectId::from_title_author(&c.title, author);
    if let Some(asin) = c
        .metadata_extras
        .get("audible_asin")
        .and_then(|v| v.as_str())
    {
        id = id.with_asin(asin);
    }
    if let Some(isbn) = c.metadata_extras.get("isbn13").and_then(|v| v.as_str()) {
        id = id.with_isbn13(isbn);
    }
    id
}
