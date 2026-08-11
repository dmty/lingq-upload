use std::path::Path;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use specta::Type;

mod error;
mod provider;
mod whisper_like;

pub use error::{TranscribeError, TranscribeErrorKind};
pub use provider::{PricingHint, ProviderCatalog, ProviderDescriptor};
pub use whisper_like::WhisperLikeTranscriber;

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
