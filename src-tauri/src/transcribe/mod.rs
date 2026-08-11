use std::path::Path;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use specta::Type;
use tempfile::TempDir;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::core::audio::{self, AudioError, AudioTrack, EncoderSettings, TranscodeReport};
use crate::core::project::Project;
use crate::error::AppError;

mod align;
mod error;
mod provider;
pub mod sample;
mod whisper_like;

pub use align::{
    normalize_for_alignment, title_match, transcript_match_head, transcript_match_tail,
    AlignSource, AlignmentMatch, BoundaryResult, ChapterCandidate, DetectedRange,
};
pub use error::{TranscribeError, TranscribeErrorKind};
pub use provider::{provider_language_hint, PricingHint, ProviderCatalog, ProviderDescriptor};
pub use sample::{
    AlignmentConfig, NoTranscriptReason, SamplePlan, SampleSide, SampleWindow, SideSamplePlan,
};
pub use whisper_like::WhisperLikeTranscriber;

#[derive(Debug, Error)]
pub enum SamplePlanningError {
    #[error("no transcript: {0:?}")]
    NoTranscript(NoTranscriptReason),
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error(transparent)]
    Resolve(#[from] AppError),
}

pub async fn resolve_and_plan_sample_windows(
    project: &Project,
    config: &AlignmentConfig,
) -> Result<SamplePlan, SamplePlanningError> {
    let tracks = crate::core::job::resolve_audio_tracks(project).await?;
    probe_and_plan_sample_windows(&tracks, config).await
}

async fn probe_and_plan_sample_windows(
    tracks: &[AudioTrack],
    config: &AlignmentConfig,
) -> Result<SamplePlan, SamplePlanningError> {
    let Some(head_track) = tracks.first() else {
        return Err(SamplePlanningError::NoTranscript(NoTranscriptReason::Empty));
    };
    let tail_track = tracks.last().expect("non-empty tracks");
    let head_duration = audio::probe_duration(&head_track.path).await?;
    let tail_duration = if head_track.path == tail_track.path {
        head_duration
    } else {
        audio::probe_duration(&tail_track.path).await?
    };

    sample::plan_sample_windows(tracks, (head_duration, tail_duration), config)
        .map_err(SamplePlanningError::NoTranscript)
}

pub struct ExtractedSample {
    _temp_dir: TempDir,
    path: std::path::PathBuf,
    report: TranscodeReport,
}

impl ExtractedSample {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn report(&self) -> &TranscodeReport {
        &self.report
    }
}

pub async fn extract_sample(
    window: &SampleWindow,
    cancel: &CancellationToken,
) -> Result<ExtractedSample, AudioError> {
    if cancel.is_cancelled() {
        return Err(AudioError::Cancelled);
    }

    let temp_dir = tempfile::tempdir()?;
    let side = match window.side {
        SampleSide::Head => "head",
        SampleSide::Tail => "tail",
    };
    let path = temp_dir
        .path()
        .join(format!("{side}-{}.mp3", window.attempt));
    let encoder = EncoderSettings {
        bitrate: "64k".into(),
        sample_rate: 16_000,
        channels: 1,
    };
    let report = audio::transcode(
        &window.path,
        &path,
        &encoder,
        Some((window.start_sec, window.end_sec)),
    )
    .await?;

    // spawn_blocking codec work completes before this post-check and cleanup.
    if cancel.is_cancelled() {
        return Err(AudioError::Cancelled);
    }

    Ok(ExtractedSample {
        _temp_dir: temp_dir,
        path,
        report,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscribeProviderId {
    #[default]
    Groq,
    OpenAi,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, Type)]
pub struct TranscribeOpts {
    pub language: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub struct Transcript {
    pub text: String,
}

pub trait Transcriber: Send + Sync {
    fn provider_id(&self) -> TranscribeProviderId;
    fn transcribe(
        &self,
        audio: &Path,
        opts: &TranscribeOpts,
    ) -> BoxFuture<'_, Result<Transcript, TranscribeError>>;
}
