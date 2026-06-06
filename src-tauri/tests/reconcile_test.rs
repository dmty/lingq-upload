use std::fs;
use std::path::Path;

use lingq_upload_lib::core::library::{reconcile, LibraryIndex, INDEX_FILENAME, INDEX_SCHEMA_V1};
use lingq_upload_lib::core::store::InMemoryProjectStore;
use lingq_upload_lib::ingest::IngestRegistry;
use tempfile::TempDir;

const OPF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>Crossover Book</dc:title>
    <dc:creator opf:role="aut">Same Author</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
</package>"#;

fn seed_root(root: &Path) {
    let calibre = root.join("Same Author").join("Crossover Book");
    fs::create_dir_all(&calibre).unwrap();
    fs::write(calibre.join("metadata.opf"), OPF).unwrap();
    fs::write(calibre.join("book.epub"), b"PK\x03\x04").unwrap();

    let libation = root
        .join("Same Author")
        .join("Crossover Book [B0CROSS01]");
    fs::create_dir_all(&libation).unwrap();
    fs::write(libation.join("audio.m4b"), b"fake").unwrap();
}

#[tokio::test]
async fn reconcile_writes_library_index() {
    let tmp = TempDir::new().unwrap();
    seed_root(tmp.path());

    let registry = IngestRegistry::default();
    let store = InMemoryProjectStore::new();
    let report = reconcile(&registry, &store, tmp.path()).await.unwrap();

    let idx_path = tmp.path().join(INDEX_FILENAME);
    assert!(idx_path.is_file(), "library.index.json written");
    let raw = fs::read_to_string(&idx_path).unwrap();
    let idx: LibraryIndex = serde_json::from_str(&raw).unwrap();
    assert_eq!(idx.schema_version, INDEX_SCHEMA_V1);

    // Both sources produce candidates pointing at the same logical book.
    assert!(
        !report.created.is_empty() || !report.merged.is_empty(),
        "report has entries"
    );
}

#[tokio::test]
async fn reconcile_idempotent_against_empty_root() {
    let tmp = TempDir::new().unwrap();
    let registry = IngestRegistry::default();
    let store = InMemoryProjectStore::new();
    let first = reconcile(&registry, &store, tmp.path()).await.unwrap();
    let second = reconcile(&registry, &store, tmp.path()).await.unwrap();
    assert_eq!(first.created.len(), 0);
    assert_eq!(first.merged.len(), 0);
    assert_eq!(second.created.len(), 0);
    assert_eq!(second.merged.len(), 0);
}
