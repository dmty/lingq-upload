pub mod json;
pub mod memory;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::core::epub::ChapterId;
use crate::core::identity::ProjectId;
use crate::core::matcher::{MappingError, MappingOp, MappingState};
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
    #[error("mapping op_id stale: server={server} expected={expected}")]
    MappingStaleOp { server: u64, expected: u64 },
    #[error("mapping error: {0}")]
    Mapping(MappingError),
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
    /// Replace the project's skipped-chapter set with `skipped_ids`.
    /// Atomic on the JSON store (tempfile + fsync + rename, AD-022) and
    /// serialised against concurrent writers to the same project so a
    /// read-modify-write race cannot drop the older edit.
    fn set_selection(
        &self,
        id: &ProjectId,
        skipped_ids: &[ChapterId],
    ) -> Result<(), StoreError>;
    /// Apply `op` to the project's `MappingState` atomically: load, gate on
    /// `expected_op_id == state.op_id + 1`, apply, persist — all under the
    /// per-project write lock so concurrent callers cannot race the RMW.
    /// Returns either the new state, a stale-op signal, or the underlying
    /// `MappingError` so callers can preserve the discriminant on the wire.
    fn apply_mapping_op(
        &self,
        id: &ProjectId,
        op: MappingOp,
        expected_op_id: u64,
    ) -> Result<MappingState, StoreError>;
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

/// Canonical on-disk form for a skipped-chapter set: sorted by inner string,
/// deduped. Lifted to a free fn so both stores share one definition.
pub(crate) fn canonicalise_selection(ids: &[ChapterId]) -> Vec<ChapterId> {
    let mut v: Vec<ChapterId> = ids.to_vec();
    v.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    v.dedup();
    v
}
