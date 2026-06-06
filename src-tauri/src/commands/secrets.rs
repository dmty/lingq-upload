use crate::error::AppError;
use crate::secrets::{RealKeyring, SecretsStore};

fn store() -> SecretsStore {
    SecretsStore::new(Box::new(RealKeyring::new()))
}

#[tauri::command]
#[specta::specta]
pub fn cmd_save_lingq_key(key: String) -> Result<(), AppError> {
    // Length-only audit trail — never log the value.
    tracing::info!(chars = key.chars().count(), "saving lingq api key");
    store().save_key(&key)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn cmd_load_lingq_key() -> Result<Option<String>, AppError> {
    Ok(store().load_key()?)
}

#[tauri::command]
#[specta::specta]
pub fn cmd_clear_lingq_key() -> Result<(), AppError> {
    tracing::info!("clearing lingq api key");
    store().clear_key()?;
    Ok(())
}
