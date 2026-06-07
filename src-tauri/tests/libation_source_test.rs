use std::fs;
use std::path::Path;

use lingq_upload_lib::ingest::{AudioSource, IngestSource, LibationFolderSource, TextSource};
use tempfile::TempDir;

const SIDECAR_JSON: &str = r#"{
  "title": "Kafka on the Shore",
  "asin": "B0ABCDEFGH",
  "authors": [{"name": "Haruki Murakami"}],
  "chapters": [
    {"title": "Chapter 1", "start_offset_ms": 0,     "length_ms": 600000},
    {"title": "Chapter 2", "start_offset_ms": 600000,"length_ms": 700000}
  ]
}"#;

fn setup_libation(root: &Path) {
    // Single-file with sidecar.
    let single = root
        .join("Haruki Murakami")
        .join("Kafka on the Shore [B0ABCDEFGH]");
    fs::create_dir_all(&single).unwrap();
    fs::write(single.join("Kafka on the Shore [B0ABCDEFGH].m4b"), b"fake").unwrap();
    fs::write(
        single.join("Kafka on the Shore [B0ABCDEFGH].json"),
        SIDECAR_JSON,
    )
    .unwrap();

    // Multi-file, no sidecar.
    let multi = root.join("Some Author").join("Multi Book [B0IJKLMNOP]");
    fs::create_dir_all(&multi).unwrap();
    fs::write(multi.join("Multi Book [B0IJKLMNOP] - 01.m4a"), b"fake").unwrap();
    fs::write(multi.join("Multi Book [B0IJKLMNOP] - 02.m4a"), b"fake").unwrap();
}

#[tokio::test]
async fn scan_walks_libation_layout() {
    let tmp = TempDir::new().unwrap();
    setup_libation(tmp.path());

    let src = LibationFolderSource;
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
        Some("B0ABCDEFGH")
    );
    assert!(matches!(
        kafka.audio_source,
        Some(AudioSource::SingleFile(_))
    ));
    assert!(matches!(kafka.text_source, TextSource::Missing));

    let manifest = kafka.chapter_manifest.as_ref().expect("sidecar parsed");
    assert_eq!(manifest.chapters.len(), 2);
    assert_eq!(manifest.chapters[0].title, "Chapter 1");
    assert!((manifest.chapters[0].start_sec - 0.0).abs() < 0.001);
    assert!((manifest.chapters[1].start_sec - 600.0).abs() < 0.001);

    let multi = candidates
        .iter()
        .find(|c| c.title.contains("Multi"))
        .unwrap();
    assert!(multi.chapter_manifest.is_none(), "no sidecar → None");
    assert!(matches!(multi.audio_source, Some(AudioSource::Folder(_))));
    assert_eq!(
        multi
            .metadata_extras
            .get("audible_asin")
            .and_then(|v| v.as_str()),
        Some("B0IJKLMNOP")
    );
}

#[tokio::test]
async fn empty_root_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let src = LibationFolderSource;
    let candidates = src.scan(tmp.path()).await.unwrap();
    assert!(candidates.is_empty());
}

#[tokio::test]
async fn extracts_last_asin_and_keeps_annotated_bracket_in_title() {
    let tmp = TempDir::new().unwrap();
    let book = tmp
        .path()
        .join("Author X")
        .join("Title [Annotated] [B0ABCDEFGH]");
    fs::create_dir_all(&book).unwrap();
    fs::write(book.join("audio.m4b"), b"fake").unwrap();

    let src = LibationFolderSource;
    let cs = src.scan(tmp.path()).await.unwrap();
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].title, "Title [Annotated]");
    assert_eq!(
        cs[0]
            .metadata_extras
            .get("audible_asin")
            .and_then(|v| v.as_str()),
        Some("B0ABCDEFGH")
    );
}

#[tokio::test]
async fn sidecar_stem_match_beats_stray_metadata_json() {
    let tmp = TempDir::new().unwrap();
    let book = tmp.path().join("Author").join("Book [B0ABCDEFGH]");
    fs::create_dir_all(&book).unwrap();
    fs::write(book.join("Book [B0ABCDEFGH].m4b"), b"fake").unwrap();
    // Real sidecar with stem match.
    fs::write(
        book.join("Book [B0ABCDEFGH].json"),
        r#"{"chapters":[{"title":"Real","start_offset_ms":0,"length_ms":1000}]}"#,
    )
    .unwrap();
    // Stray metadata that also has chapters and would lose by stem-match.
    fs::write(
        book.join("metadata.json"),
        r#"{"chapters":[{"title":"Stray","start_offset_ms":0,"length_ms":1000}]}"#,
    )
    .unwrap();

    let src = LibationFolderSource;
    let cs = src.scan(tmp.path()).await.unwrap();
    let manifest = cs[0].chapter_manifest.as_ref().unwrap();
    assert_eq!(manifest.chapters[0].title, "Real");
}

#[tokio::test]
async fn cover_prefers_stem_with_asin_then_lexical_order() {
    let tmp = TempDir::new().unwrap();
    let book = tmp.path().join("Author").join("Book [B0ABCDEFGH]");
    fs::create_dir_all(&book).unwrap();
    fs::write(book.join("Book [B0ABCDEFGH].m4b"), b"fake").unwrap();
    // Two .jpg files: only one has an ASIN stem; that one must win even
    // though its filename sorts later than the no-ASIN one.
    fs::write(book.join("aaa-no-asin.jpg"), b"fake").unwrap();
    fs::write(book.join("Book [B0ABCDEFGH].jpg"), b"fake").unwrap();

    let src = LibationFolderSource;
    let cs = src.scan(tmp.path()).await.unwrap();
    let cover = cs[0].cover_path.as_ref().unwrap();
    assert!(
        cover
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .contains("B0ABCDEFGH"),
        "got {cover:?}"
    );
}
