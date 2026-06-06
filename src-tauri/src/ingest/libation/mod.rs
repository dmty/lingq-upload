pub mod sidecar;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::future::{self, BoxFuture};

use super::{AudioSource, Candidate, ChapterEntry, IngestError, IngestSource, TextSource};

pub struct LibationFolderSource;

impl Default for LibationFolderSource {
    fn default() -> Self {
        Self
    }
}

impl LibationFolderSource {
    pub const ID: &'static str = "libation";

    fn scan_sync(&self, root: &Path) -> Result<Vec<Candidate>, IngestError> {
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for author_ent in walk_dirs(root)? {
            for book_ent in walk_dirs(&author_ent)? {
                let (audio, kind) = collect_audio(&book_ent)?;
                if audio.is_empty() {
                    continue;
                }
                let folder = book_ent.file_name().and_then(|s| s.to_str()).unwrap_or("");
                let asin = extract_asin(folder);
                let title = strip_asin_suffix(folder).trim().to_string();
                let authors = vec![
                    author_ent
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string(),
                ];

                let audio_stem = match &kind {
                    AudioKind::SingleFile(p) => p
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string()),
                    AudioKind::Folder => None,
                };
                let chapter_manifest =
                    sidecar::load_for_book_with_stem(&book_ent, audio_stem.as_deref());

                let mut extras: HashMap<String, serde_json::Value> = HashMap::new();
                if let Some(a) = asin.clone() {
                    extras.insert("audible_asin".into(), serde_json::Value::String(a));
                }

                let audio_source = Some(match kind {
                    AudioKind::SingleFile(p) => AudioSource::SingleFile(p),
                    AudioKind::Folder => AudioSource::Folder(book_ent.clone()),
                });

                out.push(Candidate {
                    source_id: Self::ID.into(),
                    title,
                    authors,
                    language: None,
                    series: None,
                    cover_path: find_cover(&book_ent),
                    text_source: TextSource::Missing,
                    audio_source,
                    chapter_manifest,
                    metadata_extras: extras,
                });
            }
        }
        out.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(out)
    }
}

fn walk_dirs(p: &Path) -> Result<Vec<PathBuf>, IngestError> {
    let mut out = Vec::new();
    for ent in std::fs::read_dir(p).map_err(|e| IngestError::Io(e.to_string()))? {
        let ent = ent.map_err(|e| IngestError::Io(e.to_string()))?;
        if ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            out.push(ent.path());
        }
    }
    out.sort();
    Ok(out)
}

enum AudioKind {
    SingleFile(PathBuf),
    Folder,
}

fn collect_audio(dir: &Path) -> Result<(Vec<PathBuf>, AudioKind), IngestError> {
    let mut found: Vec<PathBuf> = Vec::new();
    for ent in std::fs::read_dir(dir).map_err(|e| IngestError::Io(e.to_string()))? {
        let ent = ent.map_err(|e| IngestError::Io(e.to_string()))?;
        let p = ent.path();
        match p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()) {
            Some(ref s) if s == "m4b" || s == "m4a" => found.push(p),
            _ => {}
        }
    }
    found.sort();
    let kind = if found.len() == 1 {
        AudioKind::SingleFile(found[0].clone())
    } else {
        AudioKind::Folder
    };
    Ok((found, kind))
}

/// Scan all `[...]` groups in `folder` and return the offset and value of the
/// *last* one whose content matches `B0[A-Z0-9]{8}` (Audible's ASIN shape).
/// Other bracketed annotations like `[Annotated]` or `[Unabridged]` are
/// ignored. This lets `extract_asin` and `strip_asin_suffix` share scanning.
fn find_asin_group(folder: &str) -> Option<(usize, String)> {
    let bytes = folder.as_bytes();
    let mut last: Option<(usize, String)> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(rel) = bytes[i + 1..].iter().position(|&b| b == b']') {
                let inner = &folder[i + 1..i + 1 + rel];
                if is_asin_shape(inner) {
                    last = Some((i, inner.to_string()));
                }
                i = i + 1 + rel + 1;
                continue;
            }
        }
        i += 1;
    }
    last
}

fn is_asin_shape(s: &str) -> bool {
    s.len() == 10
        && s.starts_with("B0")
        && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

fn extract_asin(folder: &str) -> Option<String> {
    find_asin_group(folder).map(|(_, asin)| asin)
}

fn strip_asin_suffix(folder: &str) -> &str {
    match find_asin_group(folder) {
        Some((at, _)) => folder[..at].trim_end(),
        None => folder,
    }
}

fn find_cover(dir: &Path) -> Option<PathBuf> {
    for name in ["cover.jpg", "cover.jpeg", "cover.png"] {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    // Libation cover is `<title> [ASIN].jpg`. Prefer the .jpg whose stem
    // contains an ASIN; otherwise fall back to the first .jpg in lexical
    // order so behaviour is deterministic across filesystems.
    let mut jpgs: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("jpg"))
                .unwrap_or(false)
            {
                jpgs.push(p);
            }
        }
    }
    jpgs.sort();
    if let Some(asin_match) = jpgs.iter().find(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| extract_asin(s).is_some())
            .unwrap_or(false)
    }) {
        return Some(asin_match.clone());
    }
    jpgs.into_iter().next()
}

#[allow(dead_code)]
fn parse_libation_chapters(chapters: &[serde_json::Value]) -> Vec<ChapterEntry> {
    chapters
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
        .collect()
}

impl IngestSource for LibationFolderSource {
    fn id(&self) -> &'static str {
        Self::ID
    }
    fn label(&self) -> &'static str {
        "Libation"
    }
    fn scan<'a>(
        &'a self,
        root: &'a Path,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, IngestError>> {
        let r = self.scan_sync(root);
        Box::pin(future::ready(r))
    }
    fn enrich<'a>(
        &'a self,
        _c: &'a mut Candidate,
    ) -> BoxFuture<'a, Result<(), IngestError>> {
        Box::pin(future::ready(Ok(())))
    }
}

#[cfg(test)]
mod asin_tests {
    use super::*;

    #[test]
    fn extracts_asin_from_trailing_group() {
        assert_eq!(
            extract_asin("Kafka on the Shore [B0ABCDEFGH]").as_deref(),
            Some("B0ABCDEFGH"),
        );
    }

    #[test]
    fn ignores_non_asin_brackets() {
        assert_eq!(extract_asin("Title [Annotated]").as_deref(), None);
        assert_eq!(extract_asin("Plain Title").as_deref(), None);
    }

    #[test]
    fn picks_last_asin_when_multiple() {
        assert_eq!(
            extract_asin("Series [B0AAAAAAAA] - Book 1 [B0BBBBBBBB]").as_deref(),
            Some("B0BBBBBBBB"),
        );
    }

    #[test]
    fn strip_keeps_non_asin_brackets() {
        assert_eq!(
            strip_asin_suffix("Title [Annotated] [B0ABCDEFGH]"),
            "Title [Annotated]",
        );
    }

    #[test]
    fn strip_returns_input_when_no_asin() {
        assert_eq!(strip_asin_suffix("Title [Annotated]"), "Title [Annotated]");
        assert_eq!(strip_asin_suffix("Plain Title"), "Plain Title");
    }

    #[test]
    fn round_trip_title_and_asin() {
        let folder = "Title [Annotated] [B0XXXXXXX1]";
        assert_eq!(strip_asin_suffix(folder), "Title [Annotated]");
        assert_eq!(extract_asin(folder).as_deref(), Some("B0XXXXXXX1"));
    }

    #[test]
    fn lowercase_asin_is_rejected() {
        assert_eq!(extract_asin("Title [b0abcdefgh]"), None);
    }
}
