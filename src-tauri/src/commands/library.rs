use std::sync::Arc;

use crate::core::library::{load_or_rebuild, LibraryIndex, INDEX_FILENAME};
use crate::core::store::{InMemoryProjectStore, ProjectStore};
use crate::error::AppError;

use tauri::Manager;

/// List the library. Loads `library.index.json` if present, else rebuilds from
/// the project store.
#[tauri::command]
#[specta::specta]
pub async fn cmd_library_list(app: tauri::AppHandle) -> Result<LibraryIndex, AppError> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());
    let idx_path = root.join(INDEX_FILENAME);
    let idx = load_or_rebuild(&idx_path, store.as_ref())
        .map_err(|e| AppError::Other(format!("library: {e}")))?;
    Ok(idx)
}
