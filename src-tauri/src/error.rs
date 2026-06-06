use serde::Serialize;
use specta::Type;
use thiserror::Error;

#[derive(Error, Debug, Serialize, Type)]
#[serde(tag = "kind", content = "message")]
#[allow(dead_code)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}
