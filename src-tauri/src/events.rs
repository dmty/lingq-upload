use serde::Serialize;
use specta::Type;
use uuid::Uuid;

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
    Result {
        job_id: Uuid,
        ok: bool,
        payload: serde_json::Value,
    },
    Cancelled {
        job_id: Uuid,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(seq: &[JobEvent]) -> Result<(), &'static str> {
        let mut seen_started = false;
        let mut seen_terminal = false;
        for ev in seq {
            match ev {
                JobEvent::Started { .. } => {
                    if seen_started {
                        return Err("duplicate Started");
                    }
                    if seen_terminal {
                        return Err("Started after terminal");
                    }
                    seen_started = true;
                }
                JobEvent::Progress { .. } | JobEvent::Log { .. } => {
                    if !seen_started {
                        return Err("Progress/Log before Started");
                    }
                    if seen_terminal {
                        return Err("Progress/Log after terminal");
                    }
                }
                JobEvent::Result { .. } | JobEvent::Cancelled { .. } => {
                    if !seen_started {
                        return Err("terminal before Started");
                    }
                    if seen_terminal {
                        return Err("duplicate terminal");
                    }
                    seen_terminal = true;
                }
            }
        }
        Ok(())
    }

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
        assert_eq!(validate(&seq), Err("Progress/Log before Started"));
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
        assert_eq!(validate(&seq), Err("Progress/Log after terminal"));
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
}
