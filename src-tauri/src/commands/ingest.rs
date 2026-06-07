use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::ingest::{
    CalibreLibrarySource, Candidate, IngestSource, LibationFolderSource, ManualSource,
};

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

/// Scan a Calibre or Libation library root and return all candidates.
///
/// `source` is `"calibre"` or `"libation"`.
#[tauri::command]
#[specta::specta]
pub async fn cmd_ingest_scan(source: String, root: String) -> Result<Vec<Candidate>, AppError> {
    let root_path = Path::new(&root);
    match source.as_str() {
        "calibre" => CalibreLibrarySource.scan(root_path).await.map_err(AppError::from),
        "libation" => LibationFolderSource.scan(root_path).await.map_err(AppError::from),
        other => Err(AppError::Other(format!("unknown source: {other}"))),
    }
}
