use std::path::PathBuf;

use lingq_upload_lib::ingest::{
    AudioSource, IngestError, IngestRegistry, IngestSource, ManualSource, TextSource,
};

#[test]
fn from_files_defaults_title_from_audio_stem() {
    let epub = PathBuf::from("/tmp/ch1.xhtml");
    let audio = PathBuf::from("/tmp/ch1.m4a");

    let candidate = ManualSource::from_files(epub.clone(), audio.clone(), "ja", None)
        .expect("manual candidate");

    assert_eq!(candidate.source_id, "manual");
    assert_eq!(candidate.title, "ch1");
    assert_eq!(candidate.authors, Vec::<String>::new());
    assert_eq!(candidate.language.as_deref(), Some("ja"));
    assert!(candidate.series.is_none());
    assert!(candidate.cover_path.is_none());
    assert_eq!(candidate.text_source, TextSource::Epub(epub));
    assert_eq!(candidate.audio_source, Some(AudioSource::SingleFile(audio)));
    assert!(candidate.chapter_manifest.is_none());
    assert!(candidate.metadata_extras.is_empty());
}

#[test]
fn from_files_explicit_title_overrides_default() {
    let epub = PathBuf::from("/tmp/book.xhtml");
    let audio = PathBuf::from("/tmp/track.mp3");

    let candidate = ManualSource::from_files(epub, audio, "ru", Some("Custom".to_string()))
        .expect("manual candidate");

    assert_eq!(candidate.title, "Custom");
    assert_eq!(candidate.language.as_deref(), Some("ru"));
}

#[tokio::test]
async fn scan_and_enrich_return_not_supported() {
    let source = ManualSource;

    let scan_result = source.scan(std::path::Path::new("/tmp")).await;
    assert!(matches!(scan_result, Err(IngestError::NotSupported)));

    let mut candidate = ManualSource::from_files(
        PathBuf::from("/tmp/a.xhtml"),
        PathBuf::from("/tmp/a.m4a"),
        "ja",
        None,
    )
    .expect("manual candidate");
    let enrich_result = source.enrich(&mut candidate).await;
    assert!(matches!(enrich_result, Err(IngestError::NotSupported)));
}

#[test]
fn default_registry_includes_manual() {
    let registry = IngestRegistry::default();
    let ids: Vec<&'static str> = registry.iter().map(|s| s.id()).collect();
    assert!(ids.contains(&"manual"));
}
