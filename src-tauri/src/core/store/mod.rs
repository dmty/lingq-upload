pub mod json;
pub mod memory;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::core::identity::ProjectId;
use crate::core::project::{ChapterReceipt, Project, ProjectSummary};

pub use json::{JsonProjectStore, ListHealth};
pub use memory::InMemoryProjectStore;

#[derive(Debug, Error, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "message")]
pub enum StoreError {
    #[error("io error at {path}: {message}")]
    Io { path: PathBuf, message: String },
    #[error("corrupt JSON at {path}: {message}")]
    Corrupt { path: PathBuf, message: String },
    #[error("not found: {key}")]
    NotFound { key: String },
    #[error("index {index} out of bounds (len {len})")]
    OutOfBounds { index: usize, len: usize },
}

pub trait ProjectStore: Send + Sync {
    fn put(&self, p: &Project) -> Result<(), StoreError>;
    fn get(&self, id: &ProjectId) -> Result<Option<Project>, StoreError>;
    fn list(&self) -> Result<Vec<ProjectSummary>, StoreError>;
    fn patch_chapter(
        &self,
        id: &ProjectId,
        index: usize,
        receipt: ChapterReceipt,
    ) -> Result<(), StoreError>;
}

/// Filesystem-safe rendering of an identifier (e.g. `ProjectId::join_key()`).
/// Strong-key keys carry `:` which is illegal in Windows path segments; this
/// helper substitutes anything outside `[A-Za-z0-9._-]` with `_`.
pub fn safe_path_segment(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '.' | '_' => c,
            _ => '_',
        })
        .collect()
}
