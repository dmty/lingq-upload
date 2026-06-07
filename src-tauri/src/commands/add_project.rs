use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;

use crate::core::identity::ProjectId;
use crate::core::library::{candidate_to_id, rebuild_from_store, write_atomic, INDEX_FILENAME};
use crate::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::ingest::Candidate;

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

fn build_project(
    candidate: &Candidate,
    language: String,
    collection_title: String,
) -> Project {
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
    }
}

fn rebuild_library_index(
    app: &tauri::AppHandle,
    store: &dyn ProjectStore,
) -> Result<(), AppError> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    let idx = rebuild_from_store(store)
        .map_err(|e| AppError::Other(format!("library rebuild: {e}")))?;
    write_atomic(&idx, &root.join(INDEX_FILENAME))
        .map_err(|e| AppError::Other(format!("library write: {e}")))?;
    Ok(())
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
    let project = build_project(&candidate, language, collection_title);
    let id = project.id.clone();

    if let Some(existing) = store
        .get(&id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
    {
        return Ok(CreateProjectResult::Conflict {
            existing: existing.id.clone(),
            conflict_title: existing.settings.collection_title,
        });
    }

    store
        .put(&project)
        .map_err(|e| AppError::Other(format!("store.put: {e}")))?;
    rebuild_library_index(&app, store.inner().as_ref())?;

    Ok(CreateProjectResult::Created { id })
}

/// Resolve a create-project conflict by user choice.
///
/// - `Replace` writes blindly (same as legacy behavior).
/// - `Skip` returns the existing project's id without writing.
/// - `NewProject` appends `" (copy)"` to the title (loops until unique)
///   so the derived `content_hash` differs, then writes.
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
            let project = build_project(&candidate, language, collection_title);
            let id = project.id.clone();
            store
                .put(&project)
                .map_err(|e| AppError::Other(format!("store.put: {e}")))?;
            rebuild_library_index(&app, store.inner().as_ref())?;
            Ok(id)
        }
        ConflictResolution::Skip => {
            let probe = build_project(&candidate, language, collection_title);
            match store
                .get(&probe.id)
                .map_err(|e| AppError::Other(format!("store.get: {e}")))?
            {
                Some(existing) => Ok(existing.id),
                None => Err(AppError::Other(
                    "skip requested but no existing project found".into(),
                )),
            }
        }
        ConflictResolution::NewProject => {
            let mut title = format!("{collection_title} (copy)");
            loop {
                let project = build_project(&candidate, language.clone(), title.clone());
                let exists = store
                    .get(&project.id)
                    .map_err(|e| AppError::Other(format!("store.get: {e}")))?
                    .is_some();
                if !exists {
                    let id = project.id.clone();
                    store
                        .put(&project)
                        .map_err(|e| AppError::Other(format!("store.put: {e}")))?;
                    rebuild_library_index(&app, store.inner().as_ref())?;
                    return Ok(id);
                }
                title.push_str(" (copy)");
            }
        }
    }
}
