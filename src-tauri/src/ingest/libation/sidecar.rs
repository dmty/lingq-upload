use std::path::Path;

use crate::ingest::{ChapterEntry, ChapterManifest};

/// Find and parse the Libation JSON sidecar inside a book directory.
/// Convention: `<book>.json` sibling of the .m4b file(s).
pub fn load_for_book(dir: &Path) -> Option<ChapterManifest> {
    let rd = std::fs::read_dir(dir).ok()?;
    for ent in rd.flatten() {
        let p = ent.path();
        if p.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("json"))
            .unwrap_or(false)
        {
            if let Ok(raw) = std::fs::read_to_string(&p) {
                if let Some(m) = parse_sidecar(&raw) {
                    return Some(m);
                }
            }
        }
    }
    None
}

pub fn parse_sidecar(raw: &str) -> Option<ChapterManifest> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let chapters = v.get("chapters")?.as_array()?;
    let entries: Vec<ChapterEntry> = chapters
        .iter()
        .filter_map(|c| {
            let title = c.get("title")?.as_str()?.to_string();
            let start_ms = c.get("start_offset_ms").and_then(|v| v.as_u64()).unwrap_or(0);
            let length_ms = c.get("length_ms").and_then(|v| v.as_u64()).unwrap_or(0);
            Some(ChapterEntry {
                title,
                start_sec: start_ms as f64 / 1000.0,
                end_sec: Some((start_ms + length_ms) as f64 / 1000.0),
            })
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    Some(ChapterManifest { chapters: entries })
}
