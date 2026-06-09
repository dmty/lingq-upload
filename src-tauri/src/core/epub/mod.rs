pub mod parse;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

pub use parse::{parse_epub, HeadingStrategy};

/// Position of a chapter within a project's text.
///
/// `Body` is the default; `FrontMatter` and `BackMatter` flag preface /
/// epilogue chapters so the UI can preselect them as skipped.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ChapterKind {
    #[default]
    Body,
    FrontMatter,
    BackMatter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
pub struct Chapter {
    pub order: usize,
    pub title: String,
    pub body: String,
    /// True when the user has opted to exclude this chapter from upload.
    /// The job runner treats `skipped` chapters as if they were never
    /// present: no parse, no transcode, no LingQ import.
    #[serde(default)]
    pub skipped: bool,
    #[serde(default)]
    pub kind: ChapterKind,
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
