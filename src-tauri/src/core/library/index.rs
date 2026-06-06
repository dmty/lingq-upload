use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::core::identity::ProjectId;
use crate::core::store::{ProjectStore, StoreError};

pub const INDEX_SCHEMA_V1: u32 = 1;
pub const INDEX_FILENAME: &str = "library.index.json";

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct LibraryEntry {
    pub id: ProjectId,
    pub title: String,
    pub language: String,
    pub completed_lesson_count: usize,
    pub receipt_count: usize,
    pub mtime: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct LibraryIndex {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub entries: Vec<LibraryEntry>,
}

#[derive(Debug, Error, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "message")]
pub enum LibraryError {
    #[error("store: {0}")]
    Store(String),
    #[error("io: {0}")]
    Io(String),
}

impl From<StoreError> for LibraryError {
    fn from(e: StoreError) -> Self {
        LibraryError::Store(e.to_string())
    }
}

/// Load the index from disk if valid; otherwise rebuild from the store.
///
/// Corrupt index → silent full rebuild (per AC2).
pub fn load_or_rebuild(
    index_path: &Path,
    store: &dyn ProjectStore,
) -> Result<LibraryIndex, LibraryError> {
    if let Ok(bytes) = fs::read(index_path) {
        if let Ok(idx) = serde_json::from_slice::<LibraryIndex>(&bytes) {
            if idx.schema_version == INDEX_SCHEMA_V1 {
                return Ok(idx);
            }
        }
    }
    rebuild_from_store(store)
}

fn rebuild_from_store(store: &dyn ProjectStore) -> Result<LibraryIndex, LibraryError> {
    let summaries = store.list()?;
    let entries = summaries
        .into_iter()
        .map(|s| LibraryEntry {
            id: s.id,
            title: s.title,
            language: s.language,
            completed_lesson_count: s.completed_lesson_count,
            receipt_count: s.receipt_count,
            mtime: None,
        })
        .collect();
    Ok(LibraryIndex {
        schema_version: INDEX_SCHEMA_V1,
        generated_at: Utc::now(),
        entries,
    })
}

/// Atomic tempfile + rename write.
pub fn write_atomic(idx: &LibraryIndex, path: &Path) -> Result<(), LibraryError> {
    let parent = path
        .parent()
        .ok_or_else(|| LibraryError::Io("path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|e| LibraryError::Io(e.to_string()))?;
    let bytes = serde_json::to_vec_pretty(idx).map_err(|e| LibraryError::Io(e.to_string()))?;
    let tmp: PathBuf = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| LibraryError::Io(e.to_string()))?;
        f.write_all(&bytes)
            .map_err(|e| LibraryError::Io(e.to_string()))?;
        f.sync_all().map_err(|e| LibraryError::Io(e.to_string()))?;
    }
    fs::rename(&tmp, path).map_err(|e| LibraryError::Io(e.to_string()))?;
    Ok(())
}
