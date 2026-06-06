use std::sync::Arc;

use chrono::Utc;
use tauri::Manager;

use crate::core::identity::ProjectId;
use crate::core::matcher::{MismatchCondition, MismatchResponse};
use crate::core::project::MatcherDecision;
use crate::core::store::{JsonProjectStore, ProjectStore};
use crate::error::AppError;

/// Record the user's matcher decision and advance the project.
#[tauri::command]
#[specta::specta]
pub async fn cmd_matcher_resolve(
    app: tauri::AppHandle,
    project_id: ProjectId,
    condition: MismatchCondition,
    response: MismatchResponse,
    chapter_count: usize,
    track_count: usize,
) -> Result<(), AppError> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    let store: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(root));
    let mut project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    project.matcher_decision = Some(MatcherDecision {
        condition,
        response,
        chapter_count,
        track_count,
        user_overrode: true,
        decided_at: Utc::now(),
    });
    store
        .put(&project)
        .map_err(|e| AppError::Other(format!("store.put: {e}")))?;
    Ok(())
}
