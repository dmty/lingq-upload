pub mod detect;
pub mod kobo;
pub mod parse;

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

pub use detect::{detect_vendor, EpubVendor, VendorDetection};
pub use kobo::KoboStrategy;
pub use parse::{parse_epub, parse_epub_bytes, HeadingStrategy};

/// Open the EPUB at `path` and run vendor detection. Convenience wrapper used
/// by the orchestrator so the chosen vendor can be logged on `JobEvent::Started`
/// before parsing kicks off.
pub fn autodetect_vendor(path: &Path) -> Result<VendorDetection, EpubError> {
    let bytes = std::fs::read(path)?;
    autodetect_vendor_bytes(&bytes)
}

/// In-memory variant. The orchestrator slurps the file once and feeds both
/// detection and parse through this so we don't reopen + re-build the zip
/// twice on the hot path.
pub fn autodetect_vendor_bytes(bytes: &[u8]) -> Result<VendorDetection, EpubError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| EpubError::Zip(e.to_string()))?;
    detect_vendor(&mut zip)
}

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
/// EPUB parsers build the id via [`ChapterId::from_chapter_parts`] so the same
/// EPUB bytes produce the same id set across runs. Non-EPUB ingest paths
/// (loose files, manifests) fall back to [`ChapterId::from_order`] — those
/// chapter sets are anchored by file system layout, not a heading strategy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type, Default)]
pub struct ChapterId(pub String);

impl fmt::Display for ChapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl ChapterId {
    /// Order-anchored id for ingest sources without a heading strategy
    /// (loose-files, chapter manifests).
    pub fn from_order(order: usize) -> Self {
        Self(format!("idx:{order}"))
    }

    /// Deterministic id derived from `(strategy, spine_index, title_norm)`.
    /// The same EPUB bytes parsed by the same strategy always yield the same
    /// id, so persisted selections survive a re-parse.
    pub fn from_chapter_parts(strategy: &str, spine_index: usize, title: &str) -> Self {
        let title_norm = normalize_title(title);
        let mut h = Sha256::new();
        h.update(strategy.as_bytes());
        h.update(b"|");
        h.update(spine_index.to_string().as_bytes());
        h.update(b"|");
        h.update(title_norm.as_bytes());
        let digest = h.finalize();
        let hex16 = hex::encode(digest)[..16].to_string();
        Self(format!("{strategy}:{spine_index}:{hex16}"))
    }
}

pub(crate) fn normalize_title(s: &str) -> String {
    let nfkc: String = s.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_space = false;
    for c in lower.chars() {
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
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
