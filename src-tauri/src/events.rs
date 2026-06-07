use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::core::matcher::{MismatchCondition, MismatchResponse};

#[derive(Serialize, Type, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Stage {
    Transcoding,
    Uploading,
    Parsing,
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
    },
}

/// Public state-machine snapshot used by `JobEmitter`. Mirrors what
/// `validate(&[JobEvent])` would compute, but cheap to update incrementally.
#[derive(Default, Clone, Copy)]
struct EventState {
    seen_started: bool,
    seen_terminal: bool,
}

impl EventState {
    fn step(self, ev: &JobEvent) -> Result<Self, &'static str> {
        let mut next = self;
        match ev {
            JobEvent::Started { .. } => {
                if next.seen_started {
                    return Err("duplicate Started");
                }
                if next.seen_terminal {
                    return Err("Started after terminal");
                }
                next.seen_started = true;
            }
            JobEvent::StageChanged { .. }
            | JobEvent::Progress { .. }
            | JobEvent::Log { .. }
            | JobEvent::ChapterDone { .. } => {
                if !next.seen_started {
                    return Err("non-Started before Started");
                }
                if next.seen_terminal {
                    return Err("non-terminal after terminal");
                }
            }
            JobEvent::Result { .. }
            | JobEvent::Cancelled { .. }
            | JobEvent::NeedsMatch { .. } => {
                if !next.seen_started {
                    return Err("terminal before Started");
                }
                if next.seen_terminal {
                    return Err("duplicate terminal");
                }
                next.seen_terminal = true;
            }
        }
        Ok(next)
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
    Ok(())
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

    pub fn started(&mut self, stage: Stage) {
        self.emit(JobEvent::Started {
            job_id: self.job_id,
            stage,
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
    pub fn needs_match(
        &mut self,
        title: String,
        chapters: usize,
        tracks: usize,
        condition: MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
    ) {
        self.emit(JobEvent::NeedsMatch {
            job_id: self.job_id,
            title,
            chapters,
            tracks,
            condition,
            options,
            preselect,
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
            },
            JobEvent::Started {
                job_id: id,
                stage: Stage::Uploading,
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
            },
            JobEvent::NeedsMatch {
                job_id: id,
                title: "Book".into(),
                chapters: 5,
                tracks: 7,
                condition: MismatchCondition::CountOff,
                options: vec![MismatchResponse::PairAccept, MismatchResponse::Cancel],
                preselect: MismatchResponse::PairAccept,
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
            },
            JobEvent::NeedsMatch {
                job_id: id,
                title: "Book".into(),
                chapters: 5,
                tracks: 7,
                condition: MismatchCondition::CountOff,
                options: vec![MismatchResponse::PairAccept, MismatchResponse::Cancel],
                preselect: MismatchResponse::PairAccept,
            },
            JobEvent::Progress {
                job_id: id,
                pct: 0.5,
                message: None,
            },
        ];
        assert_eq!(validate(&seq), Err("non-terminal after terminal"));
    }
}
