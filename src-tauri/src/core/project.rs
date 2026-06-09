use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::core::audio::AbsorbPolicy;
use crate::core::epub::ChapterId;
use crate::core::identity::ProjectId;
use crate::core::matcher::{MismatchCondition, MismatchResponse};
use crate::ingest::{AudioSource, ChapterManifest, SeriesRef, TextSource};

pub const SCHEMA_V1: u32 = 1;

/// Persisted lifecycle stage of a Project. Monotonic — see `Project::advance`.
///
/// Distinct from `events::Stage`, which is the verb of an in-flight job
/// (transcoding, uploading, parsing). The two enums are non-interchangeable.
#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, Type, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStage {
    #[default]
    New,
    Parsed,
    Mapped,
    /// Reserved slot between Mapped and Uploaded. The current orchestrator
    /// interleaves transcode + upload per chapter and jumps Mapped → Done at
    /// end-of-loop. This variant lights up when the carver introduces a
    /// distinct pre-upload transcode pass.
    Transcoded,
    Uploaded,
    Done,
}

#[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
#[error("invalid stage transition: {from:?} -> {to:?}")]
pub struct StageError {
    pub from: ProjectStage,
    pub to: ProjectStage,
}

fn schema_v1() -> u32 {
    SCHEMA_V1
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ProjectSources {
    pub text: TextSource,
    #[serde(default)]
    pub audio: Option<AudioSource>,
    #[serde(default)]
    pub chapter_manifest: Option<ChapterManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ProjectSettings {
    pub language: String,
    pub collection_title: String,
    #[serde(default = "default_level")]
    pub level: u8,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_level() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct MatcherDecision {
    pub condition: MismatchCondition,
    pub response: MismatchResponse,
    pub chapter_count: usize,
    pub track_count: usize,
    #[serde(default)]
    pub user_overrode: bool,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ChapterReceipt {
    pub chapter_index: usize,
    #[serde(default)]
    pub track_index: Option<usize>,
    #[serde(default)]
    pub lesson_id: Option<i64>,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub uploaded_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct Project {
    #[serde(default = "schema_v1")]
    pub schema_version: u32,
    pub id: ProjectId,
    pub sources: ProjectSources,
    pub settings: ProjectSettings,
    #[serde(default)]
    pub receipts: Vec<ChapterReceipt>,
    #[serde(default)]
    pub queue_cursor: usize,
    #[serde(default)]
    pub completed_lesson_ids: Vec<i64>,
    #[serde(default)]
    pub matcher_decision: Option<MatcherDecision>,
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
    pub stage: ProjectStage,
    #[serde(default)]
    pub last_transition_at: Option<DateTime<Utc>>,
    /// Chapter ids the user opted out of uploading. Replaced wholesale
    /// by `ProjectStore::set_selection`. A chapter already uploaded
    /// (carries a `lesson_id` in `receipts`) is not retroactively deleted
    /// from LingQ when added here — selection only gates not-yet-uploaded
    /// chapters in the run loop.
    #[serde(default)]
    pub skipped_chapters: Vec<ChapterId>,
    /// How chapter-divider silence is folded into neighbouring tracks at
    /// carve time. Default `Forward` preserves legacy behaviour.
    #[serde(default)]
    pub absorb_policy: AbsorbPolicy,
}

impl Project {
    /// Minimal Project value for tests: typed sources, empty receipts, all
    /// option fields defaulted. New tests should reach for this; the existing
    /// field-by-field literals still work but break on every field churn.
    #[doc(hidden)]
    pub fn new_test(id: ProjectId, title: &str) -> Self {
        use std::path::PathBuf as PB;
        Project {
            schema_version: SCHEMA_V1,
            id,
            sources: ProjectSources {
                text: TextSource::Epub(PB::from("/tmp/x.epub")),
                audio: None,
                chapter_manifest: None,
            },
            settings: ProjectSettings {
                language: "ja".into(),
                collection_title: title.into(),
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
        }
    }

    pub fn stage(&self) -> ProjectStage {
        self.stage
    }

    /// Move forward through the lifecycle. Same-stage calls are idempotent
    /// no-ops (so a crash-restart re-advance doesn't restamp). Backward
    /// movement is rejected without mutation.
    pub fn advance(&mut self, to: ProjectStage) -> Result<(), StageError> {
        if to < self.stage {
            return Err(StageError {
                from: self.stage,
                to,
            });
        }
        if to != self.stage {
            self.stage = to;
            self.last_transition_at = Some(Utc::now());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ProjectSummary {
    pub id: ProjectId,
    pub title: String,
    pub language: String,
    pub receipt_count: usize,
    pub completed_lesson_count: usize,
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
    pub queue_cursor: usize,
    #[serde(default)]
    pub has_matcher_decision: bool,
    #[serde(default)]
    pub has_audio_source: bool,
    #[serde(default)]
    pub last_receipt_degraded: bool,
    #[serde(default)]
    pub chapter_manifest_len: Option<usize>,
}

impl From<&Project> for ProjectSummary {
    fn from(p: &Project) -> Self {
        Self {
            id: p.id.clone(),
            title: p.settings.collection_title.clone(),
            language: p.settings.language.clone(),
            receipt_count: p.receipts.len(),
            completed_lesson_count: p.completed_lesson_ids.len(),
            cover_path: p.cover_path.clone(),
            authors: p.authors.clone(),
            series: p.series.clone(),
            lingq_collection_id: p.lingq_collection_id,
            last_activity_at: p.last_activity_at,
            queue_cursor: p.queue_cursor,
            has_matcher_decision: p.matcher_decision.is_some(),
            has_audio_source: p.sources.audio.is_some(),
            last_receipt_degraded: p.receipts.last().is_some_and(|r| r.degraded),
            chapter_manifest_len: p
                .sources
                .chapter_manifest
                .as_ref()
                .map(|m| m.chapters.len()),
        }
    }
}
