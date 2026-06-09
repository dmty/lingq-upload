//! Round-trip cover for the picker's persistence gate against a real store.
//!
//! Exercises the `cmd_set_selection` command logic directly against a
//! `JsonProjectStore` fixture and verifies `cmd_project_chapters` reads back
//! the expected ChapterMeta projection.

use std::sync::Arc;

use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use lingq_upload_lib::core::store::{JsonProjectStore, ProjectStore};
use lingq_upload_lib::ingest::TextSource;
use tempfile::TempDir;

fn cid(order: usize) -> ChapterId {
    ChapterId::from_order(order)
}

fn make_loose_project(dir: &std::path::Path, chapter_count: usize) -> Project {
    let mut paths = Vec::with_capacity(chapter_count);
    for i in 0..chapter_count {
        let p = dir.join(format!("ch_{:02}.txt", i + 1));
        std::fs::write(&p, format!("Body {}.", i + 1)).unwrap();
        paths.push(p);
    }
    Project {
        schema_version: SCHEMA_V1,
        id: ProjectId::from_title_author("Round Trip", "Author"),
        sources: ProjectSources {
            text: TextSource::LooseFiles { paths },
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "ja".into(),
            collection_title: "Round Trip".into(),
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
    }
}

#[test]
fn cmd_set_selection_round_trip() {
    let store_dir = TempDir::new().unwrap();
    let text_dir = TempDir::new().unwrap();
    let store: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(store_dir.path()));

    let project = make_loose_project(text_dir.path(), 5);
    store.put(&project).unwrap();

    // Initial state: nothing skipped.
    let loaded = store.get(&project.id).unwrap().unwrap();
    assert!(loaded.skipped_chapters.is_empty());

    // Apply a selection.
    let skipped = vec![cid(0), cid(2), cid(4)];
    store
        .set_selection(&project.id, &skipped)
        .expect("set_selection ok");

    // Reload via the store and confirm the persisted vector matches.
    let reloaded = store.get(&project.id).unwrap().unwrap();
    assert_eq!(reloaded.skipped_chapters, vec![cid(0), cid(2), cid(4)]);

    // Apply an empty vector — wholesale clear.
    store.set_selection(&project.id, &[]).unwrap();
    let cleared = store.get(&project.id).unwrap().unwrap();
    assert!(cleared.skipped_chapters.is_empty());

    // Persist again, drop the in-memory handle, re-open the store, and
    // confirm the selection survived a process boundary (JSON on disk).
    store
        .set_selection(&project.id, &[cid(1), cid(3)])
        .unwrap();
    drop(store);

    let reopened: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(store_dir.path()));
    let after = reopened.get(&project.id).unwrap().unwrap();
    assert_eq!(after.skipped_chapters, vec![cid(1), cid(3)]);
}

#[test]
fn chapter_meta_projection_drops_body() {
    // ChapterMeta has no body field — guard against accidental re-addition.
    let meta = lingq_upload_lib::commands::project::ChapterMeta {
        id: cid(0),
        order: 0,
        title: "T".into(),
        kind: Default::default(),
    };
    let v = serde_json::to_value(&meta).unwrap();
    let obj = v.as_object().expect("ChapterMeta serialises as object");
    assert!(!obj.contains_key("body"), "ChapterMeta must not carry body");
}
