use std::fs;
use std::path::Path;

use lingq_upload_lib::ingest::{AudioSource, IngestSource, LibationFolderSource, TextSource};
use tempfile::TempDir;

const SIDECAR_JSON: &str = r#"{
  "title": "Kafka on the Shore",
  "asin": "B0XXXXX1",
  "authors": [{"name": "Haruki Murakami"}],
  "chapters": [
    {"title": "Chapter 1", "start_offset_ms": 0,     "length_ms": 600000},
    {"title": "Chapter 2", "start_offset_ms": 600000,"length_ms": 700000}
  ]
}"#;

fn setup_libation(root: &Path) {
    // Single-file with sidecar.
    let single = root.join("Haruki Murakami").join("Kafka on the Shore [B0XXXXX1]");
    fs::create_dir_all(&single).unwrap();
    fs::write(single.join("Kafka on the Shore [B0XXXXX1].m4b"), b"fake").unwrap();
    fs::write(single.join("Kafka on the Shore [B0XXXXX1].json"), SIDECAR_JSON).unwrap();

    // Multi-file, no sidecar.
    let multi = root.join("Some Author").join("Multi Book [B0XXXXX2]");
    fs::create_dir_all(&multi).unwrap();
    fs::write(multi.join("Multi Book [B0XXXXX2] - 01.m4a"), b"fake").unwrap();
    fs::write(multi.join("Multi Book [B0XXXXX2] - 02.m4a"), b"fake").unwrap();
}

#[tokio::test]
async fn scan_walks_libation_layout() {
    let tmp = TempDir::new().unwrap();
    setup_libation(tmp.path());

    let src = LibationFolderSource::default();
    let candidates = src.scan(tmp.path()).await.unwrap();
    assert_eq!(candidates.len(), 2);

    let kafka = candidates
        .iter()
        .find(|c| c.title.contains("Kafka"))
        .unwrap();
    assert_eq!(
        kafka
            .metadata_extras
            .get("audible_asin")
            .and_then(|v| v.as_str()),
        Some("B0XXXXX1")
    );
    assert!(matches!(kafka.audio_source, Some(AudioSource::SingleFile(_))));
    assert!(matches!(kafka.text_source, TextSource::Missing));

    let manifest = kafka.chapter_manifest.as_ref().expect("sidecar parsed");
    assert_eq!(manifest.chapters.len(), 2);
    assert_eq!(manifest.chapters[0].title, "Chapter 1");
    assert!((manifest.chapters[0].start_sec - 0.0).abs() < 0.001);
    assert!((manifest.chapters[1].start_sec - 600.0).abs() < 0.001);

    let multi = candidates.iter().find(|c| c.title.contains("Multi")).unwrap();
    assert!(multi.chapter_manifest.is_none(), "no sidecar → None");
    assert!(matches!(multi.audio_source, Some(AudioSource::Folder(_))));
    assert_eq!(
        multi
            .metadata_extras
            .get("audible_asin")
            .and_then(|v| v.as_str()),
        Some("B0XXXXX2")
    );
}

#[tokio::test]
async fn empty_root_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let src = LibationFolderSource::default();
    let candidates = src.scan(tmp.path()).await.unwrap();
    assert!(candidates.is_empty());
}
