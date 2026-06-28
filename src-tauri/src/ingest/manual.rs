use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::future::{self, BoxFuture};

use super::{AudioSource, Candidate, IngestError, IngestSource, TextSource};

pub struct ManualSource;

impl ManualSource {
    pub const ID: &'static str = "manual";

    pub fn from_files(
        epub: PathBuf,
        audio: PathBuf,
        lang: &str,
        title: Option<String>,
    ) -> Result<Candidate, IngestError> {
        Self::from_files_many(epub, vec![audio], lang, title)
    }

    /// Build a manual `Candidate` from an EPUB and one or more audio files.
    /// One path collapses to `AudioSource::SingleFile` so existing single-path
    /// call sites stay byte-identical; two or more paths yield
    /// `AudioSource::MultipleFiles` with order preserved as given.
    pub fn from_files_many(
        epub: PathBuf,
        audio: Vec<PathBuf>,
        lang: &str,
        title: Option<String>,
    ) -> Result<Candidate, IngestError> {
        if audio.is_empty() {
            return Err(IngestError::Parse(
                "manual source requires at least one audio file".into(),
            ));
        }

        let resolved_title = match title {
            Some(t) if !t.is_empty() => t,
            _ => audio[0]
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    IngestError::Parse(format!(
                        "cannot derive title from audio path: {}",
                        audio[0].display()
                    ))
                })?,
        };

        let audio_source = if audio.len() == 1 {
            AudioSource::SingleFile(audio.into_iter().next().expect("non-empty"))
        } else {
            AudioSource::MultipleFiles(audio)
        };

        Ok(Candidate {
            source_id: Self::ID.to_string(),
            title: resolved_title,
            authors: Vec::new(),
            language: Some(lang.to_string()),
            series: None,
            cover_path: None,
            text_source: TextSource::Epub(epub),
            audio_source: Some(audio_source),
            chapter_manifest: None,
            metadata_extras: HashMap::new(),
        })
    }
}

impl IngestSource for ManualSource {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn label(&self) -> &'static str {
        "Manual"
    }

    fn scan<'a>(&'a self, _root: &'a Path) -> BoxFuture<'a, Result<Vec<Candidate>, IngestError>> {
        Box::pin(future::ready(Err(IngestError::NotSupported)))
    }

    fn enrich<'a>(&'a self, _c: &'a mut Candidate) -> BoxFuture<'a, Result<(), IngestError>> {
        Box::pin(future::ready(Err(IngestError::NotSupported)))
    }
}
