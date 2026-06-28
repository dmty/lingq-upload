use std::sync::Arc;

use tauri::Manager;

use crate::commands::jobs::{lock_cancels, JobCancelMap};
use crate::core::identity::ProjectId;
use crate::core::library::{
    derive_status, estimated_total_chapters, list_trash, purge_project, rebuild_with_status,
    restore_project, trash_project, write_atomic, LibraryIndex, LibraryStatus, TrashEntry,
    INDEX_FILENAME,
};
use crate::core::store::ProjectStore;
use crate::error::AppError;

fn projects_root_for(app: &tauri::AppHandle) -> Result<std::path::PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))
}

/// List the library. Always rebuilds from the shared `ProjectStore` and
/// rewrites `library.index.json` as a cold-start cache. Threads the in-flight
/// `JobCancelMap` so each entry's `status` reflects whether a job is running.
#[tauri::command]
#[specta::specta]
pub async fn cmd_library_list(
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    cancels: tauri::State<'_, JobCancelMap>,
) -> Result<LibraryIndex, AppError> {
    let root = projects_root_for(&app)?;
    let idx_path = root.join(INDEX_FILENAME);

    let store_ref = store.inner().clone();
    let running: Vec<ProjectId> = {
        let guard = lock_cancels(cancels.inner());
        guard.values().map(|(pid, _)| pid.clone()).collect()
    };

    let idx = rebuild_with_status(store_ref.as_ref(), |summary| {
        let is_running = running.iter().any(|id| id == &summary.id);
        // Re-derive against the full Project so the status helper sees audio
        // source, matcher decision, receipts, and queue cursor.
        let Some(project) = store_ref.get(&summary.id).ok().flatten() else {
            return (LibraryStatus::Idle, None);
        };
        let total_chapters = estimated_total_chapters(summary);
        derive_status(&project, is_running, total_chapters)
    })
    .map_err(|e| AppError::Other(format!("library: {e}")))?;

    write_atomic(&idx, &idx_path).map_err(|e| AppError::Other(format!("library: {e}")))?;
    Ok(idx)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_trash_project(
    app: tauri::AppHandle,
    project_id: ProjectId,
) -> Result<TrashEntry, AppError> {
    let root = projects_root_for(&app)?;
    trash_project(&root, &project_id).map_err(|e| AppError::Other(format!("trash: {e}")))
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_list_trash(app: tauri::AppHandle) -> Result<Vec<TrashEntry>, AppError> {
    let root = projects_root_for(&app)?;
    list_trash(&root).map_err(|e| AppError::Other(format!("trash: {e}")))
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_restore_project(app: tauri::AppHandle, trash_id: String) -> Result<(), AppError> {
    let root = projects_root_for(&app)?;
    restore_project(&root, &trash_id).map_err(|e| AppError::Other(format!("restore: {e}")))
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_purge_project(app: tauri::AppHandle, trash_id: String) -> Result<(), AppError> {
    let root = projects_root_for(&app)?;
    purge_project(&root, &trash_id).map_err(|e| AppError::Other(format!("purge: {e}")))
}
