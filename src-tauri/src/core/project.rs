use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::core::audio::AbsorbPolicy;
use crate::core::epub::ChapterId;
use crate::core::identity::ProjectId;
use crate::core::matcher::{MappingState, MismatchCondition, MismatchResponse};
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
    /// Persisted state of the two-column mapping editor: the chapter↔track
    /// pairing, the parking lot of unpaired tracks, and a monotonic op_id.
    /// The store rejects an op whose `expected_op_id != op_id + 1`
    /// (`MappingStaleOp`), so a reloaded or duplicated submission re-syncs
    /// from this state instead of double-applying.
    #[serde(default)]
    pub mapping: Option<MappingState>,
    #[serde(default)]
    pub confirmed_at: Option<DateTime<Utc>>,
    /// User toggle: when true, the project's cover image is pushed to the
    /// LingQ collection on the next lesson upload. Default true.
    #[serde(default = "default_cover_use")]
    pub cover_use: bool,
    /// Set once after a successful LingQ image PATCH so subsequent uploads
    /// skip the cover request. Reset to false whenever `cover_path` is
    /// updated via `cmd_set_cover`.
    #[serde(default)]
    pub cover_uploaded_to_lingq: bool,
    /// Spine href of the XHTML page that hosted the extracted cover image,
    /// when known. Used by `filter_cover_chapter` to suppress the cover
    /// page from the chapter list. None for user-supplied covers or
    /// filename-heuristic extractions.
    #[serde(default)]
    pub cover_source_href: Option<String>,
}

fn default_cover_use() -> bool {
    true
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
            mapping: None,
            confirmed_at: None,
            cover_use: true,
            cover_uploaded_to_lingq: false,
            cover_source_href: None,
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
    #[serde(default)]
    pub confirmed_at: Option<DateTime<Utc>>,
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
            confirmed_at: p.confirmed_at,
        }
    }
}

/// Drop the chapter whose `spine_href` matches `cover_source_href`, if any.
/// No-op when the project has no recorded cover host (`None`) or when no
/// chapter matches (stale href after a re-parse).
pub fn filter_cover_chapter(
    chapters: Vec<crate::core::epub::Chapter>,
    cover_source_href: Option<&str>,
) -> Vec<crate::core::epub::Chapter> {
    let Some(href) = cover_source_href else {
        return chapters;
    };
    chapters
        .into_iter()
        .filter(|c| c.spine_href != href)
        .collect()
}

#[cfg(test)]
mod project_cover_tests {
    use super::*;
    use crate::core::epub::{Chapter, ChapterId, ChapterKind};

    #[test]
    fn filter_cover_chapter_drops_matching_spine_href() {
        let chapters = vec![
            Chapter {
                id: ChapterId::from_chapter_parts("k", "cover.xhtml", "Cover"),
                title: "Cover".into(),
                body: "body".into(),
                spine_href: "cover.xhtml".into(),
                kind: ChapterKind::Body,
                ..Default::default()
            },
            Chapter {
                id: ChapterId::from_chapter_parts("k", "ch1.xhtml", "Ch 1"),
                title: "Ch 1".into(),
                body: "body".into(),
                spine_href: "ch1.xhtml".into(),
                kind: ChapterKind::Body,
                ..Default::default()
            },
        ];
        let out = filter_cover_chapter(chapters.clone(), Some("cover.xhtml"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].spine_href, "ch1.xhtml");

        let unchanged = filter_cover_chapter(chapters.clone(), None);
        assert_eq!(unchanged.len(), 2);

        let no_match = filter_cover_chapter(chapters, Some("does-not-exist.xhtml"));
        assert_eq!(no_match.len(), 2);
    }

    #[test]
    fn project_defaults_for_new_cover_fields() {
        // Round-trip via JSON to confirm #[serde(default)] holds for legacy rows.
        let json = r#"{"schema_version":1,"id":{"content_hash":"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"},"sources":{"text":{"kind":"epub","value":"/tmp/x.epub"}},"settings":{"language":"ja","collection_title":"t","level":1,"tags":[]}}"#;
        let p: Project = serde_json::from_str(json).unwrap();
        assert!(p.cover_use);
        assert!(!p.cover_uploaded_to_lingq);
        assert!(p.cover_source_href.is_none());
    }
}

#[cfg(test)]
mod confirmed_at_tests {
    use super::*;
    use crate::core::identity::ProjectId;

    #[test]
    fn new_test_defaults_confirmed_at_to_none() {
        let id = ProjectId::from_title_author("T", "A");
        let p = Project::new_test(id, "T");
        assert!(p.confirmed_at.is_none());
    }

    #[test]
    fn confirmed_at_round_trips_through_json() {
        let id = ProjectId::from_title_author("T", "A");
        let mut p = Project::new_test(id, "T");
        let now = chrono::Utc::now();
        p.confirmed_at = Some(now);
        let json = serde_json::to_string(&p).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.confirmed_at, Some(now));
    }

    #[test]
    fn deserialize_legacy_json_without_confirmed_at_defaults_to_none() {
        let json = r#"{
          "schema_version": 1,
          "id": {"content_hash": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"},
          "sources": {"text": {"kind": "epub", "value": "/tmp/x.epub"}},
          "settings": {"language": "ja", "collection_title": "T", "level": 1, "tags": []}
        }"#;
        let p: Project = serde_json::from_str(json).unwrap();
        assert!(p.confirmed_at.is_none());
    }

    #[test]
    fn summary_carries_confirmed_at() {
        let id = ProjectId::from_title_author("T", "A");
        let mut p = Project::new_test(id, "T");
        let now = chrono::Utc::now();
        p.confirmed_at = Some(now);
        let s: ProjectSummary = (&p).into();
        assert_eq!(s.confirmed_at, Some(now));
    }
}
