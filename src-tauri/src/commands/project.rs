use std::sync::Arc;

use crate::core::project::Project;
use crate::core::store::ProjectStore;
use crate::error::AppError;

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
