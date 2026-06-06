pub mod lang;
pub mod opf;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::future::{self, BoxFuture};

use super::{Candidate, IngestError, IngestSource, SeriesRef, TextSource};

pub struct CalibreLibrarySource;

impl Default for CalibreLibrarySource {
    fn default() -> Self {
        Self
    }
}

impl CalibreLibrarySource {
    pub const ID: &'static str = "calibre";

    fn scan_sync(&self, root: &Path) -> Result<Vec<Candidate>, IngestError> {
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for author_ent in walk(root)? {
            for book_ent in walk(&author_ent)? {
                let opf_path = book_ent.join("metadata.opf");
                if !opf_path.is_file() {
                    continue;
                }
                let raw = std::fs::read_to_string(&opf_path)
                    .map_err(|e| IngestError::Io(e.to_string()))?;
                let meta = opf::parse_opf(&raw)
                    .map_err(|e| IngestError::Parse(format!("{}: {e}", opf_path.display())))?;
                let epub_path = find_first_with_ext(&book_ent, "epub")?;
                let cover_path = find_cover(&book_ent);
                let text_source = match epub_path {
                    Some(p) => TextSource::Epub(p),
                    None => TextSource::Missing,
                };
                let mut extras: HashMap<String, serde_json::Value> = HashMap::new();
                if let Some(isbn) = meta.isbn13.clone() {
                    extras.insert("isbn13".into(), serde_json::Value::String(isbn));
                }
                if let Some(uuid) = meta.calibre_uuid {
                    extras.insert(
                        "calibre_uuid".into(),
                        serde_json::Value::String(uuid.to_string()),
                    );
                }
                if !meta.tags.is_empty() {
                    extras.insert(
                        "tags".into(),
                        serde_json::Value::Array(
                            meta.tags
                                .iter()
                                .cloned()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                }
                out.push(Candidate {
                    source_id: Self::ID.into(),
                    title: meta.title,
                    authors: meta.authors,
                    language: meta.language.as_deref().map(lang::normalise).map(String::from),
                    series: meta.series.map(|name| SeriesRef {
                        name,
                        index: meta.series_index,
                    }),
                    cover_path,
                    text_source,
                    audio_source: None,
                    chapter_manifest: None,
                    metadata_extras: extras,
                });
            }
        }
        out.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(out)
    }
}

fn walk(p: &Path) -> Result<Vec<PathBuf>, IngestError> {
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

fn find_first_with_ext(dir: &Path, ext: &str) -> Result<Option<PathBuf>, IngestError> {
    let mut found: Vec<PathBuf> = Vec::new();
    for ent in std::fs::read_dir(dir).map_err(|e| IngestError::Io(e.to_string()))? {
        let ent = ent.map_err(|e| IngestError::Io(e.to_string()))?;
        let p = ent.path();
        if p.extension().and_then(|e| e.to_str()).map(|s| s.eq_ignore_ascii_case(ext)).unwrap_or(false) {
            found.push(p);
        }
    }
    found.sort();
    Ok(found.into_iter().next())
}

fn find_cover(dir: &Path) -> Option<PathBuf> {
    for name in ["cover.jpg", "cover.jpeg", "cover.png"] {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

impl IngestSource for CalibreLibrarySource {
    fn id(&self) -> &'static str {
        Self::ID
    }
    fn label(&self) -> &'static str {
        "Calibre"
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
