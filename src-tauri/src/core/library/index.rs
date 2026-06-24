#[cfg(test)]
use crate::core::audio::AbsorbPolicy;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::core::identity::ProjectId;
use crate::core::project::{Project, ProjectSummary};
use crate::core::store::{ProjectStore, StoreError};
use crate::ingest::SeriesRef;

pub const INDEX_SCHEMA_V1: u32 = 1;
pub const INDEX_FILENAME: &str = "library.index.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum LibraryStatus {
    Done,
    Running,
    Paused,
    NeedsMatch,
    Failed,
    #[default]
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct LibraryEntry {
    pub id: ProjectId,
    pub title: String,
    pub language: String,
    pub completed_lesson_count: usize,
    pub receipt_count: usize,
    pub mtime: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cover_path: Option<PathBuf>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub series: Option<SeriesRef>,
    #[serde(default)]
    pub lingq_collection_id: Option<i64>,
    #[serde(default)]
    pub last_activity_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub status: LibraryStatus,
    #[serde(default)]
    pub failed_reason: Option<String>,
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
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
}

impl From<StoreError> for LibraryError {
    fn from(e: StoreError) -> Self {
        LibraryError::Store(e.to_string())
    }
}

/// Build a fresh `LibraryIndex` from the store and persist it to disk.
///
/// The on-disk index is just a cold-start cache; the store is the source of
/// truth. Always rebuild here so callers see writes from other commands
/// (e.g. project creation) without manual invalidation. The cached file is
/// rewritten atomically so cross-process readers still see a consistent
/// snapshot.
pub fn load_or_rebuild(
    index_path: &Path,
    store: &dyn ProjectStore,
) -> Result<LibraryIndex, LibraryError> {
    let idx = rebuild_from_store(store)?;
    write_atomic(&idx, index_path)?;
    Ok(idx)
}

pub fn rebuild_from_store(store: &dyn ProjectStore) -> Result<LibraryIndex, LibraryError> {
    rebuild_with_status(store, |_| (LibraryStatus::Idle, None))
}

/// Build a `LibraryIndex` from the store using a caller-supplied status fn.
/// The closure sees each `ProjectSummary` and returns the derived status and
/// optional failure reason. Used by `cmd_library_list` to thread the running
/// job map without pulling tauri types into this module.
pub fn rebuild_with_status(
    store: &dyn ProjectStore,
    mut status_for: impl FnMut(&ProjectSummary) -> (LibraryStatus, Option<String>),
) -> Result<LibraryIndex, LibraryError> {
    let summaries = store.list()?;
    let entries = summaries
        .into_iter()
        .map(|s| {
            let (status, failed_reason) = status_for(&s);
            summary_to_entry(s, status, failed_reason)
        })
        .collect();
    Ok(LibraryIndex {
        schema_version: INDEX_SCHEMA_V1,
        generated_at: Utc::now(),
        entries,
    })
}

pub(crate) fn summary_to_entry(
    s: ProjectSummary,
    status: LibraryStatus,
    failed_reason: Option<String>,
) -> LibraryEntry {
    LibraryEntry {
        id: s.id,
        title: s.title,
        language: s.language,
        completed_lesson_count: s.completed_lesson_count,
        receipt_count: s.receipt_count,
        mtime: None,
        cover_path: s.cover_path,
        authors: s.authors,
        series: s.series,
        lingq_collection_id: s.lingq_collection_id,
        last_activity_at: s.last_activity_at,
        status,
        failed_reason,
    }
}

/// Derive a `LibraryStatus` for one project given its current run flag and the
/// caller's chapter count. Pure — no IO. See module docs for the precedence
/// order.
pub fn derive_status(
    project: &Project,
    running: bool,
    total_chapters: usize,
) -> (LibraryStatus, Option<String>) {
    if running {
        return (LibraryStatus::Running, None);
    }
    if project.confirmed_at.is_none() && project.receipts.is_empty() {
        return (LibraryStatus::NeedsMatch, None);
    }
    let tail = project.receipts.last();
    let degraded_tail = tail.is_some_and(|r| r.degraded);
    if degraded_tail && project.queue_cursor < total_chapters {
        let reason = format!(
            "stopped at chapter {}",
            tail.map(|r| r.chapter_index).unwrap_or(0)
        );
        return (LibraryStatus::Failed, Some(reason));
    }
    if total_chapters > 0 && project.queue_cursor >= total_chapters && !degraded_tail {
        return (LibraryStatus::Done, None);
    }
    if !project.receipts.is_empty() && project.queue_cursor < total_chapters {
        return (LibraryStatus::Paused, None);
    }
    (LibraryStatus::Idle, None)
}

/// Best-effort chapter count derived from persisted project state. Pure: no
/// filesystem reads, no parser invocations. Prefers `chapter_manifest.len()`;
/// falls back to `receipt_count` when the manifest hasn't been materialised.
pub fn estimated_total_chapters(summary: &ProjectSummary) -> usize {
    summary
        .chapter_manifest_len
        .unwrap_or(summary.receipt_count)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::identity::ProjectId;
    use crate::core::project::{ChapterReceipt, Project, ProjectSettings, ProjectSources};
    use crate::ingest::{AudioSource, TextSource};
    use std::path::PathBuf;

    fn base() -> Project {
        Project {
            schema_version: crate::core::project::SCHEMA_V1,
            id: ProjectId::from_title_author("T", "A"),
            sources: ProjectSources {
                text: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
                audio: None,
                chapter_manifest: None,
            },
            settings: ProjectSettings {
                language: "ja".into(),
                collection_title: "T".into(),
                level: 1,
                tags: vec![],
            },
            receipts: vec![],
            queue_cursor: 0,
            completed_lesson_ids: vec![],
            matcher_decision: None,
            cover_path: None,
            authors: vec![],
            series: None,
            lingq_collection_id: None,
            last_activity_at: None,
            stage: Default::default(),
            last_transition_at: None,
            skipped_chapters: vec![],
            absorb_policy: AbsorbPolicy::default(),
            mapping: None,
            confirmed_at: None,
        }
    }

    fn receipt(idx: usize, degraded: bool) -> ChapterReceipt {
        ChapterReceipt {
            chapter_index: idx,
            track_index: Some(idx),
            lesson_id: Some(100 + idx as i64),
            degraded,
            uploaded_at: Some(Utc::now()),
        }
    }

    #[test]
    fn status_running_wins() {
        let mut p = base();
        p.sources.audio = Some(AudioSource::SingleFile(PathBuf::from("/tmp/a.m4b")));
        // running=true beats every other condition, including needs_match.
        let (s, r) = derive_status(&p, true, 3);
        assert_eq!(s, LibraryStatus::Running);
        assert!(r.is_none());
    }

    #[test]
    fn status_needs_match_when_audio_without_decision() {
        let mut p = base();
        p.sources.audio = Some(AudioSource::SingleFile(PathBuf::from("/tmp/a.m4b")));
        let (s, r) = derive_status(&p, false, 5);
        assert_eq!(s, LibraryStatus::NeedsMatch);
        assert!(r.is_none());
    }

    #[test]
    fn status_failed_when_tail_degraded_with_remaining() {
        let mut p = base();
        p.receipts.push(receipt(0, false));
        p.receipts.push(receipt(1, true));
        p.queue_cursor = 2;
        let (s, r) = derive_status(&p, false, 5);
        assert_eq!(s, LibraryStatus::Failed);
        assert_eq!(r.as_deref(), Some("stopped at chapter 1"));
    }

    #[test]
    fn status_done_when_cursor_at_end_and_no_degraded_tail() {
        let mut p = base();
        p.receipts.push(receipt(0, false));
        p.receipts.push(receipt(1, false));
        p.queue_cursor = 2;
        let (s, r) = derive_status(&p, false, 2);
        assert_eq!(s, LibraryStatus::Done);
        assert!(r.is_none());
    }

    #[test]
    fn status_paused_when_some_receipts_and_more_to_go() {
        let mut p = base();
        p.receipts.push(receipt(0, false));
        p.queue_cursor = 1;
        let (s, _) = derive_status(&p, false, 5);
        assert_eq!(s, LibraryStatus::Paused);
    }

    #[test]
    fn status_idle_when_no_receipts_and_no_audio() {
        let mut p = base();
        p.confirmed_at = Some(Utc::now());
        let (s, _) = derive_status(&p, false, 0);
        assert_eq!(s, LibraryStatus::Idle);
    }

    #[test]
    fn unconfirmed_empty_project_is_needs_match() {
        let id = ProjectId::from_title_author("T", "A");
        let p = Project::new_test(id, "T");
        let (status, _) = derive_status(&p, false, 0);
        assert_eq!(status, LibraryStatus::NeedsMatch);
    }

    #[test]
    fn confirmed_empty_project_is_idle() {
        let id = ProjectId::from_title_author("T", "A");
        let mut p = Project::new_test(id, "T");
        p.confirmed_at = Some(chrono::Utc::now());
        let (status, _) = derive_status(&p, false, 0);
        assert_eq!(status, LibraryStatus::Idle);
    }

    #[test]
    fn legacy_project_with_receipts_treated_as_confirmed() {
        let id = ProjectId::from_title_author("T", "A");
        let mut p = Project::new_test(id, "T");
        // confirmed_at intentionally None — legacy data
        p.receipts = vec![ChapterReceipt {
            chapter_index: 0,
            track_index: None,
            lesson_id: Some(1),
            degraded: false,
            uploaded_at: None,
        }];
        p.queue_cursor = 1;
        let (status, _) = derive_status(&p, false, 1);
        assert_eq!(status, LibraryStatus::Done);
    }
}
