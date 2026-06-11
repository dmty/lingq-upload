use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{Chapter, EpubError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HeadingStrategy {
    Kindle,
    NavDoc,
    GenericH1,
    Kobo,
}

/// Parse an EPUB file into a Vec<Chapter>. The heading strategy is derived
/// internally via [`super::autodetect_vendor`] — callers cannot bypass
/// detection. Empty/whitespace-only chapters are dropped. Body text is
/// `strip_ruby`-clean.
pub fn parse_epub(path: &Path) -> Result<Vec<Chapter>, EpubError> {
    let bytes = std::fs::read(path)?;
    parse_epub_bytes(&bytes)
}

/// In-memory variant of [`parse_epub`]. Used by the orchestrator after the
/// file has already been slurped + decoded for vendor detection — avoids a
/// second `open + ZipArchive::new` round-trip.
pub fn parse_epub_bytes(bytes: &[u8]) -> Result<Vec<Chapter>, EpubError> {
    let strategy = strategy_from_bytes(bytes);
    parse_epub_with_strategy(bytes, strategy)
}

fn strategy_from_bytes(bytes: &[u8]) -> HeadingStrategy {
    match super::autodetect_vendor_bytes(bytes) {
        Ok(d) if d.vendor == super::EpubVendor::Kobo => HeadingStrategy::Kobo,
        _ => HeadingStrategy::Kindle,
    }
}

/// Explicit-strategy entrypoint. Production code routes through
/// [`parse_epub_bytes`] / [`parse_epub`] so detection always runs;
/// this form exists for tests and snapshots that pin a strategy
/// regardless of detection.
pub fn parse_epub_with_strategy(
    bytes: &[u8],
    strategy: HeadingStrategy,
) -> Result<Vec<Chapter>, EpubError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| EpubError::Zip(e.to_string()))?;
    match strategy {
        HeadingStrategy::Kobo => super::kobo::parse_from_zip(&mut zip),
        HeadingStrategy::Kindle | HeadingStrategy::NavDoc | HeadingStrategy::GenericH1 => {
            super::kindle::parse_from_zip(&mut zip)
        }
    }
}
