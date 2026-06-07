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
        let resolved_title = match title {
            Some(t) if !t.is_empty() => t,
            _ => audio
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    IngestError::Parse(format!(
                        "cannot derive title from audio path: {}",
                        audio.display()
                    ))
                })?,
        };

        Ok(Candidate {
            source_id: Self::ID.to_string(),
            title: resolved_title,
            authors: Vec::new(),
            language: Some(lang.to_string()),
            series: None,
            cover_path: None,
            text_source: TextSource::Epub(epub),
            audio_source: Some(AudioSource::SingleFile(audio)),
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
