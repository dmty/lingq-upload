use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

pub mod ruby;

pub use ruby::strip_ruby;

/// Next char at `bytes[i]`. Caller guarantees `bytes` is valid UTF-8 and `i`
/// is on a char boundary. Lead-byte stepping (don't revert to `from_utf8` —
/// quadratic per chapter).
pub(crate) fn next_char_at(bytes: &[u8], i: usize) -> char {
    let width = match bytes[i] {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 1,
    };
    let end = (i + width).min(bytes.len());
    unsafe { std::str::from_utf8_unchecked(&bytes[i..end]) }
        .chars()
        .next()
        .unwrap_or('\u{FFFD}')
}

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
