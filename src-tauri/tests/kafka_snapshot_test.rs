//! Snapshot test for 海辺のカフカ 下巻.
//!
//! Skip-if-missing: the EPUB is a personal Kindle decrypt and gitignored.
//! Run `scripts/fixtures/seed-epub.sh` (and `extract_shimo.sh`) to populate
//! the fixture; the test then asserts chapter count and per-chapter shape via
//! an `insta` JSON snapshot.

use std::path::PathBuf;

use lingq_upload_lib::core::epub::{parse_epub, HeadingStrategy};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("epub")
        .join("kafka_shimo.epub")
}

#[test]
fn kafka_shimo_kindle_snapshot() {
    let path = fixture_path();
    if !path.exists() {
        eprintln!(
            "skipping kafka snapshot — fixture missing at {}; run scripts/fixtures/seed-epub.sh",
            path.display()
        );
        return;
    }

    let chapters = parse_epub(&path, HeadingStrategy::Kindle).expect("parse_epub succeeded");

    assert_eq!(chapters.len(), 27, "expected 27 chapters in 下巻");
    for c in &chapters {
        assert!(
            !c.body.is_empty(),
            "chapter {} body must be non-empty",
            c.order
        );
    }

    let shape: Vec<(usize, String, usize)> = chapters
        .iter()
        .map(|c| (c.order, c.title.clone(), c.body.len()))
        .collect();

    insta::assert_json_snapshot!("kafka_shimo_chapters", shape);
}
