use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

pub mod calibre;
pub mod libation;
pub mod manual;

pub use calibre::CalibreLibrarySource;
pub use libation::LibationFolderSource;
pub use manual::ManualSource;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct SeriesRef {
    pub name: String,
    pub index: Option<f32>,
}

// Adjacently tagged: required because newtype variants (e.g. `Epub(PathBuf)`)
// are not representable as internally tagged in serde.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum TextSource {
    Epub(PathBuf),
    LooseFiles { paths: Vec<PathBuf> },
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AudioSource {
    SingleFile(PathBuf),
    Folder(PathBuf),
    LibationManifest(PathBuf),
}

/// Resolve an `AudioSource` to the ordered list of audio file paths it
/// represents. `SingleFile` and `LibationManifest` yield a single entry;
/// `Folder` yields top-level files whose extension is `m4b` / `m4a` / `mp3`,
/// case-insensitive, sorted by path. Shared between the upload command and
/// the job orchestrator so both agree on what "the audio tracks for this
/// project" means.
pub fn audio_source_paths(src: &AudioSource) -> std::io::Result<Vec<PathBuf>> {
    match src {
        AudioSource::SingleFile(p) | AudioSource::LibationManifest(p) => Ok(vec![p.clone()]),
        AudioSource::Folder(dir) => {
            let mut out = Vec::new();
            for entry in std::fs::read_dir(dir)?.flatten() {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase);
                if matches!(ext.as_deref(), Some("m4b" | "m4a" | "mp3")) {
                    out.push(p);
                }
            }
            out.sort();
            Ok(out)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ChapterEntry {
    pub title: String,
    pub start_sec: f64,
    pub end_sec: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ChapterManifest {
    pub chapters: Vec<ChapterEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Candidate {
    pub source_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub series: Option<SeriesRef>,
    pub cover_path: Option<PathBuf>,
    pub text_source: TextSource,
    pub audio_source: Option<AudioSource>,
    pub chapter_manifest: Option<ChapterManifest>,
    pub metadata_extras: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Error, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "message")]
pub enum IngestError {
    #[error("not supported")]
    NotSupported,
    #[error("io error: {0}")]
    Io(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("ingest error: {0}")]
    Other(String),
}

pub trait IngestSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn scan<'a>(&'a self, root: &'a Path) -> BoxFuture<'a, Result<Vec<Candidate>, IngestError>>;
    fn enrich<'a>(&'a self, c: &'a mut Candidate) -> BoxFuture<'a, Result<(), IngestError>>;
}

pub struct IngestRegistry {
    sources: Vec<Box<dyn IngestSource>>,
}

impl IngestRegistry {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    pub fn register(&mut self, source: Box<dyn IngestSource>) {
        self.sources.push(source);
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn IngestSource> {
        self.sources.iter().map(|s| s.as_ref())
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

impl Default for IngestRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(ManualSource));
        registry.register(Box::new(CalibreLibrarySource));
        registry.register(Box::new(LibationFolderSource));
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn single_file_yields_one_path() {
        let p = PathBuf::from("/tmp/book.m4b");
        let got = audio_source_paths(&AudioSource::SingleFile(p.clone())).unwrap();
        assert_eq!(got, vec![p]);
    }

    #[test]
    fn libation_manifest_yields_one_path() {
        let p = PathBuf::from("/tmp/libation.json");
        let got = audio_source_paths(&AudioSource::LibationManifest(p.clone())).unwrap();
        assert_eq!(got, vec![p]);
    }

    #[test]
    fn folder_lists_audio_extensions_sorted_and_skips_others() {
        let dir = tempdir().unwrap();
        for name in [
            "02.m4b",
            "01.m4b",
            "cover.jpg",
            "notes.txt",
            "track3.MP3",
            "extra.m4a",
        ] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        // Nested file must be ignored (top-level only).
        let nested = dir.path().join("inner");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("hidden.m4b"), b"x").unwrap();

        let got = audio_source_paths(&AudioSource::Folder(dir.path().to_path_buf())).unwrap();
        let names: Vec<_> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["01.m4b", "02.m4b", "extra.m4a", "track3.MP3"]);
    }

    #[test]
    fn folder_missing_dir_is_io_error() {
        let res = audio_source_paths(&AudioSource::Folder(PathBuf::from(
            "/definitely/not/a/real/dir",
        )));
        assert!(res.is_err());
    }
}
