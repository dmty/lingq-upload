pub mod add_project;
pub mod demo;
pub mod ingest;
pub mod jobs;
pub mod library;
pub mod lingq;
pub mod matcher;
pub mod ping;
pub mod project;
pub mod secrets;
pub mod upload;

use crate::error::AppError;
use crate::lingq::LanguageCode;

/// Single validation boundary for raw `lang: String` strings coming in from
/// Tauri IPC. Downstream code consumes the typed `LanguageCode`.
pub(crate) fn parse_lang(lang: &str) -> Result<LanguageCode, AppError> {
    LanguageCode::new(lang).map_err(AppError::from)
}
