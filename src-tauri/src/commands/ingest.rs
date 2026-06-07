use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;
use crate::ingest::{
    CalibreLibrarySource, Candidate, IngestSource, LibationFolderSource, ManualSource,
};

/// Bookshelf source for `cmd_ingest_scan`. Modelled as an enum so specta
/// emits a TS union and the frontend can't pass a misspelled string.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySource {
    Calibre,
    Libation,
}

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
#[tauri::command]
#[specta::specta]
pub async fn cmd_ingest_scan(
    source: LibrarySource,
    root: String,
) -> Result<Vec<Candidate>, AppError> {
    let root_path = Path::new(&root);
    match source {
        LibrarySource::Calibre => {
            CalibreLibrarySource.scan(root_path).await.map_err(AppError::from)
        }
        LibrarySource::Libation => {
            LibationFolderSource.scan(root_path).await.map_err(AppError::from)
        }
    }
}
