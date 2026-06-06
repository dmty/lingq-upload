use std::sync::Arc;

use crate::core::library::{load_or_rebuild, LibraryIndex, INDEX_FILENAME};
use crate::core::store::ProjectStore;
use crate::error::AppError;

use tauri::Manager;

/// List the library. Always rebuilds from the shared `ProjectStore` and
/// rewrites `library.index.json` as a cold-start cache.
#[tauri::command]
#[specta::specta]
pub async fn cmd_library_list(
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
) -> Result<LibraryIndex, AppError> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    let idx_path = root.join(INDEX_FILENAME);
    let idx = load_or_rebuild(&idx_path, store.inner().as_ref())
        .map_err(|e| AppError::Other(format!("library: {e}")))?;
    Ok(idx)
}
