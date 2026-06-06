use std::fs;
use std::path::Path;

use lingq_upload_lib::ingest::{
    CalibreLibrarySource, IngestRegistry, IngestSource, ManualSource, TextSource,
};
use tempfile::TempDir;

const OPF_RICH: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="uuid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>海辺のカフカ 下</dc:title>
    <dc:creator opf:role="aut">村上春樹</dc:creator>
    <dc:language>jpn</dc:language>
    <dc:subject>fiction</dc:subject>
    <dc:subject>japanese</dc:subject>
    <dc:identifier opf:scheme="ISBN">9784101001012</dc:identifier>
    <dc:identifier opf:scheme="calibre">aaaa1111-bbbb-2222-cccc-333344445555</dc:identifier>
    <meta name="calibre:series" content="海辺のカフカ"/>
    <meta name="calibre:series_index" content="2.0"/>
  </metadata>
</package>"#;

const OPF_MINIMAL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>Minimal Book</dc:title>
    <dc:creator opf:role="aut">Some Author</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
</package>"#;

const OPF_AUDIOBOOK_ONLY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>Audio Only</dc:title>
    <dc:creator opf:role="aut">Narrator</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
</package>"#;

fn setup_library(root: &Path) {
    let rich = root.join("村上春樹").join("海辺のカフカ 下");
    fs::create_dir_all(&rich).unwrap();
    fs::write(rich.join("metadata.opf"), OPF_RICH).unwrap();
    fs::write(rich.join("kafka.epub"), b"PK\x03\x04").unwrap();

    let mini = root.join("Some Author").join("Minimal Book");
    fs::create_dir_all(&mini).unwrap();
    fs::write(mini.join("metadata.opf"), OPF_MINIMAL).unwrap();
    fs::write(mini.join("minimal.epub"), b"PK\x03\x04").unwrap();

    let audio_only = root.join("Narrator").join("Audio Only");
    fs::create_dir_all(&audio_only).unwrap();
    fs::write(audio_only.join("metadata.opf"), OPF_AUDIOBOOK_ONLY).unwrap();
}

#[tokio::test]
async fn scan_walks_calibre_layout() {
    let tmp = TempDir::new().unwrap();
    setup_library(tmp.path());

    let src = CalibreLibrarySource;
    let candidates = src.scan(tmp.path()).await.unwrap();
    assert_eq!(candidates.len(), 3);

    let kafka = candidates.iter().find(|c| c.title.contains("カフカ")).unwrap();
    assert_eq!(kafka.authors, vec!["村上春樹"]);
    assert_eq!(kafka.language.as_deref(), Some("ja"));
    assert_eq!(
        kafka.series.as_ref().map(|s| s.name.as_str()),
        Some("海辺のカフカ")
    );
    assert_eq!(kafka.series.as_ref().and_then(|s| s.index), Some(2.0));
    assert!(matches!(kafka.text_source, TextSource::Epub(_)));
    assert_eq!(
        kafka
            .metadata_extras
            .get("isbn13")
            .and_then(|v| v.as_str()),
        Some("9784101001012")
    );

    let audio_only = candidates.iter().find(|c| c.title == "Audio Only").unwrap();
    assert!(matches!(audio_only.text_source, TextSource::Missing));
}

#[tokio::test]
async fn empty_root_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let src = CalibreLibrarySource;
    let candidates = src.scan(tmp.path()).await.unwrap();
    assert!(candidates.is_empty());
}

#[test]
fn registry_default_includes_calibre_and_libation_and_manual() {
    let reg = IngestRegistry::default();
    let ids: Vec<&str> = reg.iter().map(|s| s.id()).collect();
    assert!(ids.contains(&ManualSource::ID));
    assert!(ids.contains(&"calibre"));
    assert!(ids.contains(&"libation"));
    assert_eq!(reg.len(), 3);
}
