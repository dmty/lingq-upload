//! Behaviour test for the join-key lookup used by `cmd_project_load`.
//!
//! The command itself takes an `AppHandle` so we can't construct it in a
//! unit test without spinning up Tauri. Instead, replicate the lookup
//! against a real `JsonProjectStore` — that's the only logic the command
//! adds on top of the store.

use lingq_upload_lib::core::audio::AbsorbPolicy;
use std::path::PathBuf;

use chrono::Utc;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::{
    ChapterReceipt, Project, ProjectSettings, ProjectSources, SCHEMA_V1,
};
use lingq_upload_lib::core::store::{JsonProjectStore, ProjectStore};
use lingq_upload_lib::ingest::TextSource;
use tempfile::TempDir;
use uuid::Uuid;

fn sample_with(id: ProjectId, title: &str) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id,
        sources: ProjectSources {
            text: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "ja".into(),
            collection_title: title.into(),
            level: 1,
            tags: vec![],
        },
        receipts: vec![ChapterReceipt {
            chapter_index: 0,
            track_index: Some(0),
            lesson_id: Some(42),
            degraded: true,
            uploaded_at: Some(Utc::now()),
        }],
        queue_cursor: 1,
        completed_lesson_ids: vec![42],
        matcher_decision: None,
        cover_path: None,
        authors: vec![],
        series: None,
        lingq_collection_id: None,
        last_activity_at: None,
        stage: Default::default(),
        last_transition_at: None,
    skipped_chapters: vec![],
    absorb_policy: AbsorbPolicy::default(),
    mapping: None,
    }
}

fn lookup_by_key<S: ProjectStore + ?Sized>(store: &S, key: &str) -> Option<Project> {
    let id = store
        .list()
        .ok()?
        .into_iter()
        .find(|s| s.id.join_key() == key)
        .map(|s| s.id)?;
    store.get(&id).ok().flatten()
}

#[test]
fn lookup_resolves_content_hash_key() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let p = sample_with(ProjectId::from_title_author("Whisper", "Author"), "Whisper");
    store.put(&p).unwrap();

    let got = lookup_by_key(&store, &p.id.join_key()).expect("found");
    assert_eq!(got.id, p.id);
    assert_eq!(got.receipts.len(), 1);
    assert!(got.receipts[0].degraded);
}

#[test]
fn lookup_resolves_strong_key_when_present() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let p = sample_with(
        ProjectId::from_title_author("Wind", "Author").with_asin("B0TEST1234"),
        "Wind",
    );
    store.put(&p).unwrap();

    let got = lookup_by_key(&store, "asin:B0TEST1234").expect("found");
    assert_eq!(got.id.audible_asin.as_deref(), Some("B0TEST1234"));
}

#[test]
fn lookup_returns_none_for_unknown_key() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    store
        .put(&sample_with(
            ProjectId::from_title_author("Only", "Author"),
            "Only",
        ))
        .unwrap();

    assert!(lookup_by_key(&store, "ch:deadbeef").is_none());
}

#[test]
fn lookup_resolves_calibre_uuid_key() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let uuid = Uuid::new_v4();
    let p = sample_with(
        ProjectId::from_title_author("Calibre Book", "Author").with_calibre_uuid(uuid),
        "Calibre Book",
    );
    store.put(&p).unwrap();

    let got = lookup_by_key(&store, &format!("uuid:{uuid}")).expect("found");
    assert_eq!(got.id.calibre_uuid, Some(uuid));
}
