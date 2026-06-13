pub mod add_project;
pub mod demo;
pub mod files;
pub mod ingest;
pub mod jobs;
pub mod library;
pub mod lingq;
pub mod mapping;
pub mod matcher;
pub mod ping;
pub mod project;
pub mod secrets;
pub mod upload;

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::lingq::LanguageCode;

/// Single validation boundary for raw `lang: String` strings coming in from
/// Tauri IPC. Downstream code consumes the typed `LanguageCode`.
pub(crate) fn parse_lang(lang: &str) -> Result<LanguageCode, AppError> {
    LanguageCode::new(lang).map_err(AppError::from)
}

/// Resolve the per-user app data dir. Used by commands that need to read or
/// write files under the platform's `app_data` location (secrets, dev prefs).
pub(crate) fn app_data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))
}
