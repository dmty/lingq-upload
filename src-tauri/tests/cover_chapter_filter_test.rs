use std::path::Path;

use lingq_upload_lib::commands::project::{chapter_text, project_chapters_impl};
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
use lingq_upload_lib::ingest::TextSource;


fn epub_fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/epub-covers")
        .join(name)
}

fn make_epub_project(cover_source_href: Option<&str>) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id: ProjectId::from_title_author("CoverFilterSmoke", "Author"),
        sources: ProjectSources {
            text: TextSource::Epub(epub_fixture("guide-xhtml-img.epub")),
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "en".into(),
            collection_title: "CoverFilterSmoke".into(),
            level: 1,
            tags: vec![],
        },
        receipts: vec![],
        queue_cursor: 0,
        completed_lesson_ids: vec![],
        matcher_decision: None,
        cover_path: None,
        authors: vec![],
        series: None,
        lingq_collection_id: None,
        last_activity_at: None,
        stage: Default::default(),
        last_transition_at: None,
        skipped_chapters: vec![],
        absorb_policy: Default::default(),
        mapping: None,
        confirmed_at: None,
        cover_use: true,
        cover_uploaded_to_lingq: false,
        cover_source_href: cover_source_href.map(str::to_owned),
    }
}

/// When `cover_source_href` is set, the cover XHTML must not appear in the
/// chapter list returned to the UI picker.
#[test]
fn project_chapters_suppresses_cover_xhtml() {
    let store = InMemoryProjectStore::default();
    let project = make_epub_project(Some("cover.xhtml"));
    let id = project.id.clone();
    store.put(&project).unwrap();

    let chapters = project_chapters_impl(&store, &id).unwrap();
    let has_cover = chapters.iter().any(|c| c.title.to_lowercase().contains("cover"));
    assert!(
        !has_cover,
        "cover chapter must be suppressed from chapter list; got: {:?}",
        chapters.iter().map(|c| &c.title).collect::<Vec<_>>()
    );
}

/// Without `cover_source_href` the chapter list is unfiltered (control case).
#[test]
fn project_chapters_unfiltered_when_no_cover_href() {
    let store = InMemoryProjectStore::default();
    let project = make_epub_project(None);
    let id = project.id.clone();
    store.put(&project).unwrap();

    let chapters = project_chapters_impl(&store, &id).unwrap();
    // guide-xhtml-img.epub has cover.xhtml + chapter1.xhtml in spine — expect ≥2
    assert!(
        chapters.len() >= 2,
        "unfiltered list should include all spine items; got {}",
        chapters.len()
    );
}

/// `chapter_text` must also suppress the cover chapter so the UI body-fetch
/// cannot reach cover HTML by chapter id.
#[tokio::test]
async fn chapter_text_suppresses_cover_chapter() {
    let store = InMemoryProjectStore::default();
    // First parse without filter to discover the cover chapter's id.
    // cover.xhtml is first in the spine so it is order index 0.
    let project_no_filter = make_epub_project(None);
    let id_no_filter = project_no_filter.id.clone();
    store.put(&project_no_filter).unwrap();
    let chapters_all = project_chapters_impl(&store, &id_no_filter).unwrap();
    assert!(
        chapters_all.len() >= 2,
        "fixture must have at least 2 chapters (cover + body)"
    );
    // cover.xhtml is the first spine item.
    let cover_id = chapters_all[0].id.clone();

    // Now with filter active, chapter_text for the cover id must fail.
    let store2 = InMemoryProjectStore::default();
    let project_filtered = make_epub_project(Some("cover.xhtml"));
    let id_filtered = project_filtered.id.clone();
    store2.put(&project_filtered).unwrap();

    let result = chapter_text(&store2, &id_filtered, &cover_id).await;
    assert!(
        result.is_err(),
        "chapter_text for cover chapter must return Err when cover_source_href is set"
    );
}
