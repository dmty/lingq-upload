use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

pub mod ruby;

pub use ruby::strip_ruby;

#[derive(Error, Debug, Serialize, Deserialize, Type, Clone)]
#[serde(tag = "kind", content = "message")]
#[allow(dead_code)]
pub enum TextError {
    #[error("io: {0}")]
    Io(String),
}

impl From<std::io::Error> for TextError {
    fn from(e: std::io::Error) -> Self {
        TextError::Io(e.to_string())
    }
}

/// Read a UTF-8 text file for upload: strip a leading BOM if present and
/// fold to Unicode NFC. Catches BOM injection / NFC-NFD divergence regressions
/// that have historically corrupted Japanese / CJK uploads.
pub fn read_text_for_upload(path: &Path) -> Result<String, TextError> {
    let raw = std::fs::read_to_string(path).map_err(|e| TextError::Io(e.to_string()))?;
    let stripped = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
    Ok(stripped.nfc().collect())
}
