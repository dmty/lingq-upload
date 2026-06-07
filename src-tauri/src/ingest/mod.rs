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
