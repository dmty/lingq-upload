use std::path::PathBuf;

use crate::error::AppError;
use crate::ingest::{Candidate, ManualSource};

#[tauri::command]
#[specta::specta]
pub fn manual_source_from_files(
    epub: PathBuf,
    audio: PathBuf,
    lang: String,
    title: Option<String>,
) -> Result<Candidate, AppError> {
    ManualSource::from_files(epub, audio, &lang, title).map_err(AppError::from)
}
