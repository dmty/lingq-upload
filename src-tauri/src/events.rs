use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::core::epub::EpubVendor;
use crate::core::matcher::{BucketPreview, MismatchCondition, MismatchResponse};
use crate::error::AppError;
use crate::transcribe::{DetectStartResult, DetectionSink, TranscribeError};

#[derive(Serialize, Type, Clone, Copy, Debug, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Stage {
    Transcoding,
    Uploading,
    Parsing,
    DetectingStart,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DetectionPhase {
    TitleCheck,
    SampleHead,
    TranscribeHead,
    AlignHead,
    SampleTail,
    TranscribeTail,
    AlignTail,
}

#[derive(Serialize, Type, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Serialize, Type, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
#[allow(dead_code)]
pub enum JobEvent {
    Started {
        job_id: Uuid,
        stage: Stage,
        /// Vendor chosen by autodetection. Optional because non-EPUB sources
        /// don't have a vendor and older replay logs pre-date the field.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strategy: Option<EpubVendor>,
    },
    StageChanged {
        job_id: Uuid,
        stage: Stage,
    },
    Progress {
        job_id: Uuid,
        pct: f32,
        message: Option<String>,
    },
    DetectionProgress {
        job_id: Uuid,
        pct: f32,
        phase: DetectionPhase,
    },
    Log {
        job_id: Uuid,
        level: LogLevel,
        message: String,
    },
    ChapterDone {
        job_id: Uuid,
        chapter_index: usize,
        lesson_id: i64,
        degraded: bool,
    },
    Result {
        job_id: Uuid,
        ok: bool,
        payload: serde_json::Value,
    },
    Cancelled {
        job_id: Uuid,
    },
    /// Emitted when the orchestrator can't auto-pair chapters and tracks
    /// and needs the user to pick a [`MismatchResponse`]. Terminal: once
    /// emitted no further events fire for this job — the UI navigates to
    /// `/match`, the user resolves, and the next job kicks off fresh.
    NeedsMatch {
        job_id: Uuid,
        title: String,
        chapters: usize,
        tracks: usize,
        condition: MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
        /// Populated only when `condition == ManyToFew`; the proportional
        /// packer's preview rows for the Mismatch UI's SplitProportional
        /// card. `serde(default)` keeps older project.json / replay logs
        /// (written before this field existed) loadable.
        #[serde(default)]
        bucket_preview: Option<Vec<BucketPreview>>,
    },
}

/// Public state-machine snapshot used by `JobEmitter`. Mirrors what
/// `validate(&[JobEvent])` would compute, but cheap to update incrementally.
#[derive(Clone, Copy, PartialEq)]
enum JobKind {
    Standard,
    Detection,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Lifecycle {
    #[default]
    AwaitingStart,
    Running,
    Terminal,
}

#[derive(Default, Clone, Copy)]
struct EventState {
    lifecycle: Lifecycle,
    job_kind: Option<JobKind>,
    stage: Option<Stage>,
}

impl EventState {
    fn step(self, ev: &JobEvent) -> Result<Self, &'static str> {
        let mut next = self;
        match ev {
            JobEvent::Started { stage, .. } => {
                match next.lifecycle {
                    Lifecycle::AwaitingStart => {}
                    Lifecycle::Running => return Err("duplicate Started"),
                    Lifecycle::Terminal => return Err("Started after terminal"),
                }
                next.lifecycle = Lifecycle::Running;
                next.job_kind = Some(if *stage == Stage::DetectingStart {
                    JobKind::Detection
                } else {
                    JobKind::Standard
                });
                next.stage = Some(*stage);
            }
            JobEvent::StageChanged { stage, .. } => {
                next.require_running()?;
                let detection_job = next.job_kind == Some(JobKind::Detection);
                if (*stage == Stage::DetectingStart) != detection_job {
                    return Err("stage change cannot cross detection boundary");
                }
                next.stage = Some(*stage);
            }
            JobEvent::DetectionProgress { .. } => {
                next.require_running()?;
                if next.job_kind != Some(JobKind::Detection)
                    || next.stage != Some(Stage::DetectingStart)
                {
                    return Err("detection progress requires DetectingStart");
                }
            }
            JobEvent::Progress { .. } | JobEvent::Log { .. } | JobEvent::ChapterDone { .. } => {
                next.require_running()?;
            }
            JobEvent::Result { .. } | JobEvent::Cancelled { .. } => {
                return next.finish();
            }
            JobEvent::NeedsMatch { .. } => {
                let finished = next.finish()?;
                if finished.job_kind == Some(JobKind::Detection) {
                    return Err("detection terminal must be Result or Cancelled");
                }
                return Ok(finished);
            }
        }
        Ok(next)
    }

    fn require_running(self) -> Result<(), &'static str> {
        match self.lifecycle {
            Lifecycle::AwaitingStart => Err("non-Started before Started"),
            Lifecycle::Running => Ok(()),
            Lifecycle::Terminal => Err("non-terminal after terminal"),
        }
    }

    fn finish(mut self) -> Result<Self, &'static str> {
        match self.lifecycle {
            Lifecycle::AwaitingStart => Err("terminal before Started"),
            Lifecycle::Running => {
                self.lifecycle = Lifecycle::Terminal;
                Ok(self)
            }
            Lifecycle::Terminal => Err("duplicate terminal"),
        }
    }

    #[cfg(test)]
    fn complete(self) -> Result<(), &'static str> {
        (self.lifecycle == Lifecycle::Terminal)
            .then_some(())
            .ok_or("missing terminal")
    }
}

/// Whole-sequence validator. Preserved as a test helper so the contract
/// can be exercised against a hand-built event list; runtime emission now
/// uses the incremental `EventState::step` to avoid O(n²) history clones.
#[cfg(test)]
pub(crate) fn validate(seq: &[JobEvent]) -> Result<(), &'static str> {
    let mut state = EventState::default();
    for ev in seq {
        state = state.step(ev)?;
    }
    state.complete()
}

/// Single-job event emitter that enforces the validate() invariant at runtime.
///
/// In debug builds a duplicate `Started`, out-of-order Progress, or post-terminal
/// emission trips a `debug_assert!`. In release the violating event is dropped
/// (and logged) so we don't break a user's upload, but the bug is loud in tests.
///
/// Tracks state incrementally (`EventState`) rather than retaining the full
/// event history — the previous `history.clone()` per emit was O(n²) on
/// long jobs (27 chapters × ~3 events each).
pub struct JobEmitter<'a> {
    app: &'a AppHandle,
    job_id: Uuid,
    state: EventState,
}

impl<'a> JobEmitter<'a> {
    pub fn new(app: &'a AppHandle, job_id: Uuid) -> Self {
        Self {
            app,
            job_id,
            state: EventState::default(),
        }
    }

    pub fn started(&mut self, stage: Stage, strategy: Option<EpubVendor>) {
        self.emit(JobEvent::Started {
            job_id: self.job_id,
            stage,
            strategy,
        });
    }

    pub fn stage(&mut self, stage: Stage) {
        self.emit(JobEvent::StageChanged {
            job_id: self.job_id,
            stage,
        });
    }

    pub fn progress(&mut self, pct: f32, message: Option<String>) {
        self.emit(JobEvent::Progress {
            job_id: self.job_id,
            pct,
            message,
        });
    }

    pub fn detection_progress(&mut self, pct: f32, phase: DetectionPhase) {
        self.emit(JobEvent::DetectionProgress {
            job_id: self.job_id,
            pct,
            phase,
        });
    }

    pub fn chapter_done(&mut self, chapter_index: usize, lesson_id: i64, degraded: bool) {
        self.emit(JobEvent::ChapterDone {
            job_id: self.job_id,
            chapter_index,
            lesson_id,
            degraded,
        });
    }

    pub fn cancelled(&mut self) {
        self.emit(JobEvent::Cancelled {
            job_id: self.job_id,
        });
    }

    pub fn result(&mut self, ok: bool, payload: serde_json::Value) {
        self.emit(JobEvent::Result {
            job_id: self.job_id,
            ok,
            payload,
        });
    }

    /// Terminal: the orchestrator paused for user matcher input. The UI
    /// consumes this to navigate to `/match`.
    #[allow(clippy::too_many_arguments)]
    pub fn needs_match(
        &mut self,
        title: String,
        chapters: usize,
        tracks: usize,
        condition: MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
        bucket_preview: Option<Vec<BucketPreview>>,
    ) {
        self.emit(JobEvent::NeedsMatch {
            job_id: self.job_id,
            title,
            chapters,
            tracks,
            condition,
            options,
            preselect,
            bucket_preview,
        });
    }

    fn emit(&mut self, event: JobEvent) {
        match self.state.step(&event) {
            Ok(next) => self.state = next,
            Err(why) => {
                debug_assert!(false, "JobEvent invariant broken: {why}");
                tracing::error!(why = %why, "JobEvent invariant broken; dropping event");
                return;
            }
        }
        if let Err(e) = self.app.emit("job", event) {
            tracing::warn!(error = %e, "JobEvent emit dropped");
        }
    }
}

impl DetectionSink for JobEmitter<'_> {
    fn started(&mut self, job_id: Uuid) {
        debug_assert_eq!(self.job_id, job_id);
        JobEmitter::started(self, Stage::DetectingStart, None);
    }

    fn progress(&mut self, job_id: Uuid, pct: f32, phase: DetectionPhase) {
        debug_assert_eq!(self.job_id, job_id);
        self.detection_progress(pct, phase);
    }

    fn result(&mut self, job_id: Uuid, result: &DetectStartResult) {
        debug_assert_eq!(self.job_id, job_id);
        JobEmitter::result(
            self,
            true,
            serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
        );
    }

    fn error(&mut self, job_id: Uuid, error: &TranscribeError) {
        debug_assert_eq!(self.job_id, job_id);
        JobEmitter::result(
            self,
            false,
            serde_json::to_value(AppError::from(error.clone())).unwrap_or(serde_json::Value::Null),
        );
    }

    fn cancelled(&mut self, job_id: Uuid) {
        debug_assert_eq!(self.job_id, job_id);
        JobEmitter::cancelled(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_sequence_passes() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Transcoding,
                strategy: None,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 0.5,
                message: None,
            },
            JobEvent::Result {
                job_id: id,
                ok: true,
                payload: serde_json::Value::Null,
            },
        ];
        assert!(validate(&seq).is_ok());
    }

    #[test]
    fn valid_sequence_with_log_and_progress_passes() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Uploading,
                strategy: None,
            },
            JobEvent::Log {
                job_id: id,
                level: LogLevel::Info,
                message: "uploading".into(),
            },
            JobEvent::Progress {
                job_id: id,
                pct: 0.25,
                message: None,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 1.0,
                message: Some("done".into()),
            },
            JobEvent::Result {
                job_id: id,
                ok: true,
                payload: serde_json::json!({"chapters": 1}),
            },
        ];
        assert!(validate(&seq).is_ok());
    }

    #[test]
    fn duplicate_started_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Transcoding,
                strategy: None,
            },
            JobEvent::Started {
                job_id: id,
                stage: Stage::Uploading,
                strategy: None,
            },
        ];
        assert_eq!(validate(&seq), Err("duplicate Started"));
    }

    #[test]
    fn out_of_order_progress_before_started_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Progress {
                job_id: id,
                pct: 0.5,
                message: None,
            },
            JobEvent::Started {
                job_id: id,
                stage: Stage::Transcoding,
                strategy: None,
            },
        ];
        assert_eq!(validate(&seq), Err("non-Started before Started"));
    }

    #[test]
    fn terminal_before_started_fails() {
        let id = Uuid::new_v4();
        let seq = vec![JobEvent::Result {
            job_id: id,
            ok: true,
            payload: serde_json::Value::Null,
        }];
        assert_eq!(validate(&seq), Err("terminal before Started"));
    }

    #[test]
    fn duplicate_result_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Transcoding,
                strategy: None,
            },
            JobEvent::Result {
                job_id: id,
                ok: true,
                payload: serde_json::Value::Null,
            },
            JobEvent::Result {
                job_id: id,
                ok: false,
                payload: serde_json::Value::Null,
            },
        ];
        assert_eq!(validate(&seq), Err("duplicate terminal"));
    }

    #[test]
    fn progress_after_terminal_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Transcoding,
                strategy: None,
            },
            JobEvent::Result {
                job_id: id,
                ok: true,
                payload: serde_json::Value::Null,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 1.0,
                message: None,
            },
        ];
        assert_eq!(validate(&seq), Err("non-terminal after terminal"));
    }

    #[test]
    fn cancelled_counts_as_terminal() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Transcoding,
                strategy: None,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 0.5,
                message: None,
            },
            JobEvent::Cancelled { job_id: id },
        ];
        assert!(validate(&seq).is_ok());
    }

    #[test]
    fn upload_one_shot_sequence_is_valid() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Parsing,
                strategy: None,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 0.0,
                message: Some("Reading text".into()),
            },
            JobEvent::StageChanged {
                job_id: id,
                stage: Stage::Transcoding,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 0.0,
                message: Some("Transcoding audio".into()),
            },
            JobEvent::Progress {
                job_id: id,
                pct: 1.0,
                message: Some("Transcode complete".into()),
            },
            JobEvent::StageChanged {
                job_id: id,
                stage: Stage::Uploading,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 0.0,
                message: Some("Uploading to LingQ".into()),
            },
            JobEvent::Result {
                job_id: id,
                ok: true,
                payload: serde_json::json!({"lesson_id": 1}),
            },
        ];
        assert!(validate(&seq).is_ok());
    }

    #[test]
    fn cancelled_followed_by_result_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Transcoding,
                strategy: None,
            },
            JobEvent::Cancelled { job_id: id },
            JobEvent::Result {
                job_id: id,
                ok: true,
                payload: serde_json::Value::Null,
            },
        ];
        assert_eq!(validate(&seq), Err("duplicate terminal"));
    }

    #[test]
    fn needs_match_is_terminal() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Uploading,
                strategy: None,
            },
            JobEvent::NeedsMatch {
                job_id: id,
                title: "Book".into(),
                chapters: 5,
                tracks: 7,
                condition: MismatchCondition::CountOff,
                options: vec![MismatchResponse::PairAccept, MismatchResponse::Cancel],
                preselect: MismatchResponse::PairAccept,
                bucket_preview: None,
            },
        ];
        assert!(validate(&seq).is_ok());
    }

    #[test]
    fn needs_match_then_progress_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Uploading,
                strategy: None,
            },
            JobEvent::NeedsMatch {
                job_id: id,
                title: "Book".into(),
                chapters: 5,
                tracks: 7,
                condition: MismatchCondition::CountOff,
                options: vec![MismatchResponse::PairAccept, MismatchResponse::Cancel],
                preselect: MismatchResponse::PairAccept,
                bucket_preview: None,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 0.5,
                message: None,
            },
        ];
        assert_eq!(validate(&seq), Err("non-terminal after terminal"));
    }

    #[test]
    fn detection_sequence_has_one_terminal() {
        let id = Uuid::new_v4();
        let phases = [
            DetectionPhase::TitleCheck,
            DetectionPhase::SampleHead,
            DetectionPhase::TranscribeHead,
            DetectionPhase::AlignHead,
            DetectionPhase::SampleTail,
            DetectionPhase::TranscribeTail,
            DetectionPhase::AlignTail,
        ];
        let mut seq = vec![JobEvent::Started {
            job_id: id,
            stage: Stage::DetectingStart,
            strategy: None,
        }];
        seq.extend(phases.into_iter().enumerate().map(|(index, phase)| {
            JobEvent::DetectionProgress {
                job_id: id,
                pct: (index + 1) as f32 / 7.0,
                phase,
            }
        }));
        seq.push(JobEvent::Result {
            job_id: id,
            ok: true,
            payload: serde_json::Value::Null,
        });

        assert_eq!(validate(&seq), Ok(()));
    }

    #[test]
    fn detection_sequence_requires_terminal() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::DetectingStart,
                strategy: None,
            },
            JobEvent::DetectionProgress {
                job_id: id,
                pct: 0.5,
                phase: DetectionPhase::TranscribeHead,
            },
        ];

        assert_eq!(validate(&seq), Err("missing terminal"));
    }

    #[test]
    fn detection_progress_before_started_fails() {
        let id = Uuid::new_v4();
        let seq = vec![JobEvent::DetectionProgress {
            job_id: id,
            pct: 0.1,
            phase: DetectionPhase::TitleCheck,
        }];

        assert_eq!(validate(&seq), Err("non-Started before Started"));
    }

    #[test]
    fn detection_progress_requires_detecting_start() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::Parsing,
                strategy: None,
            },
            JobEvent::DetectionProgress {
                job_id: id,
                pct: 0.5,
                phase: DetectionPhase::TranscribeHead,
            },
            JobEvent::Result {
                job_id: id,
                ok: true,
                payload: serde_json::Value::Null,
            },
        ];

        assert_eq!(
            validate(&seq),
            Err("detection progress requires DetectingStart")
        );
    }

    #[test]
    fn stage_changed_cannot_cross_detection_boundary() {
        for (started, changed) in [
            (Stage::Parsing, Stage::DetectingStart),
            (Stage::DetectingStart, Stage::Parsing),
        ] {
            let id = Uuid::new_v4();
            let seq = vec![
                JobEvent::Started {
                    job_id: id,
                    stage: started,
                    strategy: None,
                },
                JobEvent::StageChanged {
                    job_id: id,
                    stage: changed,
                },
                JobEvent::Result {
                    job_id: id,
                    ok: true,
                    payload: serde_json::Value::Null,
                },
            ];

            assert_eq!(
                validate(&seq),
                Err("stage change cannot cross detection boundary")
            );
        }
    }

    #[test]
    fn detection_duplicate_started_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::DetectingStart,
                strategy: None,
            },
            JobEvent::Started {
                job_id: id,
                stage: Stage::DetectingStart,
                strategy: None,
            },
        ];

        assert_eq!(validate(&seq), Err("duplicate Started"));
    }

    #[test]
    fn detection_duplicate_terminal_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::DetectingStart,
                strategy: None,
            },
            JobEvent::Result {
                job_id: id,
                ok: false,
                payload: serde_json::Value::Null,
            },
            JobEvent::Cancelled { job_id: id },
        ];

        assert_eq!(validate(&seq), Err("duplicate terminal"));
    }

    #[test]
    fn detection_rejects_needs_match_terminal() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::DetectingStart,
                strategy: None,
            },
            JobEvent::NeedsMatch {
                job_id: id,
                title: "Book".into(),
                chapters: 5,
                tracks: 1,
                condition: MismatchCondition::ManyToOne,
                options: vec![MismatchResponse::SingleLesson, MismatchResponse::Cancel],
                preselect: MismatchResponse::SingleLesson,
                bucket_preview: None,
            },
        ];

        assert_eq!(
            validate(&seq),
            Err("detection terminal must be Result or Cancelled")
        );
    }

    #[test]
    fn detection_progress_after_terminal_fails() {
        let id = Uuid::new_v4();
        let seq = vec![
            JobEvent::Started {
                job_id: id,
                stage: Stage::DetectingStart,
                strategy: None,
            },
            JobEvent::Cancelled { job_id: id },
            JobEvent::DetectionProgress {
                job_id: id,
                pct: 1.0,
                phase: DetectionPhase::AlignTail,
            },
        ];

        assert_eq!(validate(&seq), Err("non-terminal after terminal"));
    }
}
