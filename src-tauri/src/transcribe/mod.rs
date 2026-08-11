use std::path::Path;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use specta::Type;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::core::audio::{self, AudioError, EncoderSettings, TranscodeReport};

mod error;
mod provider;
pub mod sample;
mod whisper_like;

pub use error::{TranscribeError, TranscribeErrorKind};
pub use provider::{PricingHint, ProviderCatalog, ProviderDescriptor};
pub use sample::{
    AlignmentConfig, NoTranscriptReason, SamplePlan, SampleSide, SampleWindow, SideSamplePlan,
};
pub use whisper_like::WhisperLikeTranscriber;

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
