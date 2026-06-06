use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

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
}

impl From<BatchError> for AudioError {
    fn from(b: BatchError) -> AudioError {
        match b {
            BatchError::Batch {
                source_message, ..
            } => AudioError::Io(source_message),
        }
    }
}

/// Strictly sequential transcode. Preserves book order.
///
/// `on_progress` is invoked after each completed job with `(index, report)`.
/// Failure mid-batch returns `BatchError::Batch` carrying already-completed
/// reports and the failure index — supports resume.
pub async fn transcode_batch_sequential<F>(
    jobs: Vec<TranscodeJob>,
    mut on_progress: F,
) -> Result<Vec<TranscodeReport>, BatchError>
where
    F: FnMut(usize, &TranscodeReport) + Send,
{
    let mut completed = Vec::with_capacity(jobs.len());
    for (i, job) in jobs.into_iter().enumerate() {
        match transcode(&job.src, &job.dst, &job.enc).await {
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
