use lingq_upload_lib::commands::project::chapter_text;
use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use lingq_upload_lib::core::store::{JsonProjectStore, ProjectStore};
use lingq_upload_lib::ingest::TextSource;
use tempfile::TempDir;

fn make_loose_project(text_dir: &std::path::Path) -> Project {
    let p = text_dir.join("ch_01.txt");
    std::fs::write(&p, "Chapter one body.").unwrap();
    Project {
        schema_version: SCHEMA_V1,
        id: ProjectId::from_title_author("ChapterText", "Author"),
        sources: ProjectSources {
            text: TextSource::LooseFiles { paths: vec![p] },
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "en".into(),
            collection_title: "ChapterText".into(),
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
    }
}

#[tokio::test]
async fn cmd_chapter_text_returns_one_chapter_body() {
    let store_dir = TempDir::new().unwrap();
    let text_dir = TempDir::new().unwrap();
    let store = JsonProjectStore::new(store_dir.path());
    let project = make_loose_project(text_dir.path());
    let project_id = project.id.clone();
    store.put(&project).unwrap();

    let chapter_id = ChapterId::from_order(0);
    let result = chapter_text(&store, &project_id, &chapter_id).await.unwrap();
    assert!(!result.trim().is_empty(), "chapter body must be non-empty");
}
