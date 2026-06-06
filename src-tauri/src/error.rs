use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::core::audio::AudioError;
use crate::core::text::TextError;
use crate::secrets::SecretError;

#[derive(Error, Debug, Serialize, Type)]
#[serde(tag = "kind", content = "message")]
#[allow(dead_code)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("secrets error: {0}")]
    Secrets(SecretError),
    #[error("text error: {0}")]
    Text(#[from] TextError),
    #[error("audio error: {0}")]
    Audio(#[from] AudioError),
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
