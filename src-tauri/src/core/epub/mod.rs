pub mod parse;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

pub use parse::{parse_epub, HeadingStrategy};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Chapter {
    pub order: usize,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Error, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "message")]
pub enum EpubError {
    #[error("io: {0}")]
    Io(String),
    #[error("zip: {0}")]
    Zip(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("unsupported strategy: {0}")]
    UnsupportedStrategy(String),
}

impl From<std::io::Error> for EpubError {
    fn from(e: std::io::Error) -> Self {
        EpubError::Io(e.to_string())
    }
}
