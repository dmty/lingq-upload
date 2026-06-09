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
pub use parse::{parse_epub, parse_epub_bytes, parse_epub_with_strategy, HeadingStrategy};

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

    /// Deterministic id derived from `(strategy, spine_key, title_norm)`.
    /// `spine_key` is a stable per-chapter anchor — typically the spine href
    /// — so dropping an empty chapter does not shift the ids of later ones.
    /// The same EPUB bytes parsed by the same strategy always yield the same
    /// id, so persisted selections survive a re-parse.
    pub fn from_chapter_parts(strategy: &str, spine_key: &str, title: &str) -> Self {
        let title_norm = normalize_title(title);
        let mut h = Sha256::new();
        h.update(strategy.as_bytes());
        h.update(b"|");
        h.update(spine_key.as_bytes());
        h.update(b"|");
        h.update(title_norm.as_bytes());
        let digest = h.finalize();
        let hex16 = hex::encode(digest)[..16].to_string();
        Self(format!("{strategy}:{hex16}"))
    }
}

/// Lowercased, NFKC-normalised, whitespace-collapsed title with
/// `Default_Ignorable_Code_Point` characters dropped. Strips ZWJ/ZWNJ, BOM,
/// soft hyphen, and variation selectors so a re-save that inserts or removes
/// them does not flip the chapter id hash.
pub(crate) fn normalize_title(s: &str) -> String {
    let nfkc: String = s.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_space = false;
    for c in lower.chars() {
        if is_default_ignorable(c) {
            continue;
        }
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

fn is_default_ignorable(c: char) -> bool {
    if c.is_control() {
        return true;
    }
    matches!(
        c as u32,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F
            | 0x1160
            | 0x17B4
            | 0x17B5
            | 0x180B..=0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFF8
            | 0xE0000..=0xE0FFF
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_title_drops_default_ignorables() {
        // ZWJ between the words must not affect the hash input.
        let plain = "co\u{200D}operate";
        let stripped = "cooperate";
        assert_eq!(normalize_title(plain), normalize_title(stripped));
    }

    #[test]
    fn normalize_title_drops_bom_zwnj_soft_hyphen_variation_selector() {
        let dirty = "\u{FEFF}re\u{200C}sume\u{00AD}\u{FE0F}";
        let clean = "resume";
        assert_eq!(normalize_title(dirty), normalize_title(clean));
    }

    #[test]
    fn chapter_id_hash_stable_across_spine_drift() {
        // Same spine href + title → same id regardless of position in the
        // surviving chapter list.
        let a = ChapterId::from_chapter_parts("kobo", "ch3.xhtml", "The End");
        let b = ChapterId::from_chapter_parts("kobo", "ch3.xhtml", "The End");
        assert_eq!(a, b);
        let c = ChapterId::from_chapter_parts("kobo", "ch2.xhtml", "The End");
        assert_ne!(a, c);
    }

    #[test]
    fn chapter_id_legacy_idx_string_deserialises() {
        // Persisted `skipped_chapters: ["idx:3"]` must still round-trip even
        // after the parser stopped minting `idx:N` ids — older saves drift
        // silently rather than crash-loading.
        let s = r#""idx:3""#;
        let id: ChapterId = serde_json::from_str(s).unwrap();
        assert_eq!(id, ChapterId::from_order(3));
    }
}
