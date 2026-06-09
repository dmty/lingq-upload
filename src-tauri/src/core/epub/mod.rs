pub mod parse;

use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

pub use parse::{parse_epub, HeadingStrategy};

/// Position of a chapter within a project's text.
///
/// Tagged by the heading strategy at parse time. `Body` is the default;
/// `FrontMatter` / `BackMatter` flag preface / epilogue chapters so the UI
/// can preselect them as skipped.
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

/// Stable identity for a parsed chapter.
///
/// Currently a placeholder of the form `idx:{order}`. A future heading
/// strategy will replace the inner form with a deterministic hash derived
/// from `(strategy_name, spine_index, title_normalized)`; the public API
/// keeps the same `ChapterId(String)` shape so callers do not change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type, Default)]
pub struct ChapterId(pub String);

impl fmt::Display for ChapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl ChapterId {
    /// Placeholder constructor — `idx:{order}`. Lifts to a content-hash
    /// form when the heading strategy gains stable identity.
    pub fn from_order(order: usize) -> Self {
        Self(format!("idx:{order}"))
    }
}

fn default_chapter_id() -> ChapterId {
    ChapterId(String::new())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
pub struct Chapter {
    pub order: usize,
    pub title: String,
    pub body: String,
    #[serde(default = "default_chapter_id")]
    pub id: ChapterId,
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
