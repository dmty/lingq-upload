use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::core::audio::AudioError;
use crate::core::matcher::MappingError;
use crate::core::project::StageError;
use crate::core::text::TextError;
use crate::ingest::IngestError;
use crate::lingq::LingqError;
use crate::secrets::SecretError;

#[derive(Error, Debug, Serialize, Type)]
#[serde(tag = "kind", content = "message")]
#[allow(dead_code)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("no LingQ API key configured")]
    MissingApiKey,
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("secrets error: {0}")]
    Secrets(SecretError),
    #[error("text error: {0}")]
    Text(#[from] TextError),
    #[error("audio error: {0}")]
    Audio(#[from] AudioError),
    #[error("lingq error: {0}")]
    Lingq(#[from] LingqError),
    #[error("ingest error: {0}")]
    Ingest(#[from] IngestError),
    #[error("mapping error: {0}")]
    Mapping(#[from] MappingError),
    #[error("mapping op_id stale: server={server} expected={expected}")]
    MappingStaleOp { server: u64, expected: u64 },
    #[error("{0}")]
    Other(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<SecretError> for AppError {
    fn from(e: SecretError) -> Self {
        AppError::Secrets(e)
    }
}

impl From<StageError> for AppError {
    fn from(e: StageError) -> Self {
        AppError::Other(e.to_string())
    }
}
