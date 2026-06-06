use std::path::Path;

use crate::ingest::{ChapterEntry, ChapterManifest};

/// Find and parse the Libation JSON sidecar inside a book directory.
///
/// Selection rules, in order:
/// 1. If `audio_stem` is given, prefer `<audio_stem>.json` (e.g. for
///    `Book [B0XXX].m4b` we look for `Book [B0XXX].json`). Random
///    `metadata.json` files in the same directory must not win.
/// 2. Otherwise, prefer any `.json` whose stem matches a sibling audio file's
///    stem.
/// 3. Finally, any `.json` that contains a top-level `"chapters"` array.
///
/// The function returns the first valid manifest found and ignores the rest.
pub fn load_for_book(dir: &Path) -> Option<ChapterManifest> {
    load_for_book_with_stem(dir, None)
}

pub fn load_for_book_with_stem(dir: &Path, audio_stem: Option<&str>) -> Option<ChapterManifest> {
    let mut jsons: Vec<std::path::PathBuf> = Vec::new();
    let mut audio_stems: Vec<String> = Vec::new();
    for ent in std::fs::read_dir(dir).ok()?.flatten() {
        let p = ent.path();
        let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("json") {
            jsons.push(p);
        } else if matches!(ext.to_ascii_lowercase().as_str(), "m4b" | "m4a") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                audio_stems.push(stem.to_string());
            }
        }
    }
    jsons.sort();
    audio_stems.sort();

    // 1. Exact-stem match for the named audio file.
    if let Some(stem) = audio_stem {
        if let Some(hit) = jsons.iter().find(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s == stem)
                .unwrap_or(false)
        }) {
            if let Some(m) = try_parse(hit) {
                return Some(m);
            }
        }
    }

    // 2. Stem-match against any sibling audio file.
    if !audio_stems.is_empty() {
        if let Some(hit) = jsons.iter().find(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| audio_stems.iter().any(|a| a == s))
                .unwrap_or(false)
        }) {
            if let Some(m) = try_parse(hit) {
                return Some(m);
            }
        }
    }

    // 3. Any json with a chapters array.
    for p in &jsons {
        if let Some(m) = try_parse(p) {
            return Some(m);
        }
    }
    None
}

fn try_parse(p: &Path) -> Option<ChapterManifest> {
    let raw = std::fs::read_to_string(p).ok()?;
    parse_sidecar(&raw)
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
