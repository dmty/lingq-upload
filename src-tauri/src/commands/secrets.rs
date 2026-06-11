use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use super::app_data_dir;
use crate::error::AppError;
use crate::secrets::{BackendChoice, SecretsStore};

fn store(app: &AppHandle) -> Result<SecretsStore, AppError> {
    let dir = app_data_dir(app)?;
    Ok(SecretsStore::new_default(&dir))
}

#[tauri::command]
#[specta::specta]
pub fn cmd_save_lingq_key(app: AppHandle, key: String) -> Result<(), AppError> {
    // Length-only audit trail — never log the value.
    tracing::info!(chars = key.chars().count(), "saving lingq api key");
    store(&app)?.save_key(&key)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn cmd_load_lingq_key(app: AppHandle) -> Result<Option<String>, AppError> {
    Ok(store(&app)?.load_key()?)
}

#[tauri::command]
#[specta::specta]
pub fn cmd_clear_lingq_key(app: AppHandle) -> Result<(), AppError> {
    tracing::info!("clearing lingq api key");
    store(&app)?.clear_key()?;
    Ok(())
}

/// Snapshot of the dev-secrets backend selection. `is_debug` lets the UI
/// hide the toggle in release builds; `env_override` flags when the choice
/// is forced by the `LINGQ_USE_REAL_KEYCHAIN` env var.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DevBackendInfo {
    pub is_debug: bool,
    pub current: BackendChoice,
    pub env_override: bool,
}

#[tauri::command]
#[specta::specta]
pub fn cmd_get_dev_backend(app: AppHandle) -> Result<DevBackendInfo, AppError> {
    #[cfg(debug_assertions)]
    {
        let dir = app_data_dir(&app)?;
        let env_override = std::env::var("LINGQ_USE_REAL_KEYCHAIN").is_ok();
        let prefs = crate::secrets::dev_prefs_load(&dir);
        let current = if env_override {
            BackendChoice::Keychain
        } else {
            prefs.backend.unwrap_or(BackendChoice::File)
        };
        Ok(DevBackendInfo {
            is_debug: true,
            current,
            env_override,
        })
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        Ok(DevBackendInfo {
            is_debug: false,
            current: BackendChoice::Keychain,
            env_override: false,
        })
    }
}

#[tauri::command]
#[specta::specta]
pub fn cmd_set_dev_backend(app: AppHandle, choice: BackendChoice) -> Result<(), AppError> {
    #[cfg(debug_assertions)]
    {
        let dir = app_data_dir(&app)?;
        let prefs = crate::secrets::DevPrefs {
            backend: Some(choice),
        };
        crate::secrets::dev_prefs_save(&dir, &prefs)
            .map_err(|e| AppError::Io(e.to_string()))?;
        tracing::warn!(?choice, "dev secrets backend changed via settings");
        Ok(())
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (app, choice);
        Err(AppError::Unsupported(
            "dev backend toggle is only available in debug builds".into(),
        ))
    }
}
