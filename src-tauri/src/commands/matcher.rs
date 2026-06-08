use std::sync::Arc;

use chrono::Utc;

use crate::core::identity::ProjectId;
use crate::core::job::{inspect_mismatch, MismatchInspection};
use crate::core::matcher::{allowed, MismatchCondition, MismatchResponse};
use crate::core::project::MatcherDecision;
use crate::core::store::ProjectStore;
use crate::error::AppError;

/// Record the user's matcher decision and advance the project.
#[tauri::command]
#[specta::specta]
pub async fn cmd_matcher_resolve(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    condition: MismatchCondition,
    response: MismatchResponse,
    chapter_count: usize,
    track_count: usize,
) -> Result<(), AppError> {
    let mut project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    let preselect = allowed(condition).1;
    project.matcher_decision = Some(MatcherDecision {
        condition,
        response,
        chapter_count,
        track_count,
        user_overrode: response != preselect,
        decided_at: Utc::now(),
    });
    store
        .put(&project)
        .map_err(|e| AppError::Other(format!("store.put: {e}")))?;
    Ok(())
}

/// Re-probe a project parked in `needs_match` and return the resolve payload.
///
/// Used by the Resolve UI when the user enters from the Library (no live job
/// event to consume). Returns `None` when the project pairs cleanly or has
/// already been resolved.
#[tauri::command]
#[specta::specta]
pub async fn cmd_matcher_inspect(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
) -> Result<Option<MismatchInspection>, AppError> {
    let project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    inspect_mismatch(&project).await
}
