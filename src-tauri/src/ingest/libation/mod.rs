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

                let chapter_manifest = sidecar::load_for_book(&book_ent);

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

fn extract_asin(folder: &str) -> Option<String> {
    let l = folder.rfind('[')?;
    let r = folder[l + 1..].find(']')? + l + 1;
    Some(folder[l + 1..r].to_string())
}

fn strip_asin_suffix(folder: &str) -> &str {
    match folder.rfind('[') {
        Some(i) => folder[..i].trim_end(),
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
    // Libation cover is `<title> [ASIN].jpg` — pick the first .jpg.
    if let Ok(rd) = std::fs::read_dir(dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("jpg"))
                .unwrap_or(false)
            {
                return Some(p);
            }
        }
    }
    None
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
