use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscribeErrorKind {
    ApiKey,
    Unauthorized,
    RateLimit,
    Timeout,
    Network,
    ProviderFailed,
    Audio,
}

#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize, Deserialize, Type)]
#[error("{message}")]
pub struct TranscribeError {
    pub kind: TranscribeErrorKind,
    pub message: String,
}

impl TranscribeError {
    pub(crate) fn new(kind: TranscribeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> TranscribeErrorKind {
        self.kind
    }
}
