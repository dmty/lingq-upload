use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::{transcode, AudioError, EncoderSettings, TranscodeReport};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscodeJob {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub enc: EncoderSettings,
}

#[derive(Debug, Error, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "message")]
pub enum BatchError {
    #[error("batch failed at index {failed_at}: {source_message}")]
    Batch {
        completed: Vec<TranscodeReport>,
        failed_at: usize,
        source_message: String,
    },
    #[error("batch cancelled at index {cancelled_at}")]
    Cancelled {
        completed: Vec<TranscodeReport>,
        cancelled_at: usize,
    },
}

impl From<BatchError> for AudioError {
    fn from(b: BatchError) -> AudioError {
        match b {
            BatchError::Batch {
                source_message, ..
            } => AudioError::Io(source_message),
            BatchError::Cancelled { .. } => AudioError::Cancelled,
        }
    }
}

/// Strictly sequential transcode. Preserves book order.
///
/// `on_progress` is invoked after each completed job with `(index, report)`.
/// Failure mid-batch returns `BatchError::Batch` carrying already-completed
/// reports and the failure index — supports resume.
///
/// If `token` is provided and gets cancelled, returns `BatchError::Cancelled`.
/// Cancellation is checked both between jobs (fast path) and during an
/// in-flight transcode via `tokio::select!`. When the in-flight branch loses
/// the race, the transcode future is dropped — `tokio::process::Child` with
/// `kill_on_drop(true)` then SIGKILLs ffmpeg.
pub async fn transcode_batch_sequential<F>(
    jobs: Vec<TranscodeJob>,
    token: Option<CancellationToken>,
    mut on_progress: F,
) -> Result<Vec<TranscodeReport>, BatchError>
where
    F: FnMut(usize, &TranscodeReport) + Send,
{
    let mut completed = Vec::with_capacity(jobs.len());
    for (i, job) in jobs.into_iter().enumerate() {
        if let Some(tok) = &token {
            if tok.is_cancelled() {
                return Err(BatchError::Cancelled {
                    completed,
                    cancelled_at: i,
                });
            }
        }
        let result = match &token {
            Some(tok) => {
                tokio::select! {
                    biased;
                    _ = tok.cancelled() => {
                        return Err(BatchError::Cancelled {
                            completed,
                            cancelled_at: i,
                        });
                    }
                    r = transcode(&job.src, &job.dst, &job.enc) => r,
                }
            }
            None => transcode(&job.src, &job.dst, &job.enc).await,
        };
        match result {
            Ok(report) => {
                on_progress(i, &report);
                completed.push(report);
            }
            Err(e) => {
                return Err(BatchError::Batch {
                    completed,
                    failed_at: i,
                    source_message: e.to_string(),
                });
            }
        }
    }
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pre_cancelled_token_returns_cancelled_on_first_check() {
        let token = CancellationToken::new();
        token.cancel();
        let jobs = vec![TranscodeJob {
            src: PathBuf::from("/definitely/missing.m4a"),
            dst: PathBuf::from("/tmp/never.mp3"),
            enc: EncoderSettings::default(),
        }];
        let result = transcode_batch_sequential(jobs, Some(token), |_, _| {}).await;
        match result {
            Err(BatchError::Cancelled { cancelled_at, completed }) => {
                assert_eq!(cancelled_at, 0);
                assert!(completed.is_empty());
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn batch_error_cancelled_maps_to_audio_error_cancelled() {
        let e = BatchError::Cancelled {
            completed: Vec::new(),
            cancelled_at: 0,
        };
        assert!(matches!(AudioError::from(e), AudioError::Cancelled));
    }
}
