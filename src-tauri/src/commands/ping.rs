use crate::error::AppError;

#[tauri::command]
#[specta::specta]
pub fn ping() -> Result<String, AppError> {
    Ok("pong".into())
}
