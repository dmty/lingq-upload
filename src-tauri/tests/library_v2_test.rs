//! Library v2 contract: trash subsystem, status derivation precedence, and
//! Project schema round-trips with the new fields.

use std::path::PathBuf;

use chrono::Utc;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::library::{
    list_trash, purge_project, rebuild_with_status, restore_project, trash_project, LibraryStatus,
};
use lingq_upload_lib::core::project::{
    ChapterReceipt, Project, ProjectSettings, ProjectSources, SCHEMA_V1,
};
use lingq_upload_lib::core::store::{safe_path_segment, JsonProjectStore, ProjectStore};
use lingq_upload_lib::ingest::{SeriesRef, TextSource};
use tempfile::TempDir;

fn make_project(title: &str) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id: ProjectId::from_title_author(title, "Author"),
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
        receipts: vec![],
        queue_cursor: 0,
        completed_lesson_ids: vec![],
        matcher_decision: None,
        cover_path: None,
        authors: vec!["Author".into()],
        series: None,
        lingq_collection_id: None,
        last_activity_at: None,
    }
}

#[test]
fn legacy_project_json_deserialises_without_new_fields() {
    // Anything that wasn't on disk before this change must default cleanly.
    let raw = serde_json::json!({
        "schema_version": SCHEMA_V1,
        "id": ProjectId::from_title_author("Legacy", "Author"),
        "sources": { "text": { "kind": "epub", "value": "/tmp/x.epub" } },
        "settings": { "language": "ja", "collection_title": "Legacy" }
    });
    let p: Project = serde_json::from_value(raw).unwrap();
    assert!(p.cover_path.is_none());
    assert!(p.authors.is_empty());
    assert!(p.series.is_none());
    assert!(p.lingq_collection_id.is_none());
    assert!(p.last_activity_at.is_none());
}

#[test]
fn full_shape_round_trips_equal() {
    let mut p = make_project("Full");
    p.cover_path = Some(PathBuf::from("/covers/full.jpg"));
    p.series = Some(SeriesRef {
        name: "Saga".into(),
        index: Some(2.0),
    });
    p.lingq_collection_id = Some(42);
    p.last_activity_at = Some(Utc::now());
    p.receipts.push(ChapterReceipt {
        chapter_index: 0,
        track_index: Some(0),
        lesson_id: Some(1),
        degraded: false,
        uploaded_at: Some(Utc::now()),
    });
    let json = serde_json::to_string(&p).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
}

#[test]
fn trash_round_trip_removes_then_restores() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let p = make_project("Trashy");
    store.put(&p).unwrap();
    assert_eq!(store.list().unwrap().len(), 1);

    let entry = trash_project(tmp.path(), &p.id).unwrap();
    assert_eq!(entry.project_id, p.id);
    assert_eq!(entry.title, "Trashy");
    assert_eq!(store.list().unwrap().len(), 0, "store hides trashed");

    let trashed = list_trash(tmp.path()).unwrap();
    assert_eq!(trashed.len(), 1);
    assert_eq!(trashed[0].trash_id, entry.trash_id);

    restore_project(tmp.path(), &entry.trash_id).unwrap();
    assert_eq!(store.list().unwrap().len(), 1, "restored back to store");
    assert_eq!(list_trash(tmp.path()).unwrap().len(), 0);

    // Trash again, then purge.
    let entry2 = trash_project(tmp.path(), &p.id).unwrap();
    assert_eq!(list_trash(tmp.path()).unwrap().len(), 1);
    purge_project(tmp.path(), &entry2.trash_id).unwrap();
    assert_eq!(list_trash(tmp.path()).unwrap().len(), 0);
    // .trash dir may remain but be empty; the slot under projects/ is free.
    let slot = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&p.id.join_key()));
    assert!(!slot.exists(), "purge removed the dir");
}

#[test]
fn cmd_create_project_flow_ignores_trashed_entries() {
    // Exercised at the store layer: cmd_create_project is a thin wrapper around
    // store.get (collision probe) + store.put (persist). After trash, the
    // collision probe must return None so a fresh create with the same id
    // proceeds; this mirrors AC-A5 at the command's delegated helper layer.
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let p = make_project("Reusable");
    store.put(&p).unwrap();
    let _ = trash_project(tmp.path(), &p.id).unwrap();

    // Same probe cmd_create_project uses: store.get(&id). Must return None.
    let collision = store.get(&p.id).unwrap();
    assert!(collision.is_none(), "store.get treats trashed as absent");
    assert!(
        !store.list().unwrap().iter().any(|s| s.id == p.id),
        "store.list omits trashed entries"
    );
    // Re-create succeeds (would have errored if the trashed dir still counted).
    store.put(&p).unwrap();
    assert_eq!(store.list().unwrap().len(), 1);
    assert!(
        store.get(&p.id).unwrap().is_some(),
        "recreated entry visible"
    );
}

#[test]
fn rebuilt_library_index_omits_trashed_entries() {
    // cmd_library_list disappears trashed projects because rebuild_with_status
    // walks store.list, which filters .trash. Verify at the index level.
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let a = make_project("Keep");
    let b = make_project("Drop");
    store.put(&a).unwrap();
    store.put(&b).unwrap();

    let before = rebuild_with_status(&store, |_| (LibraryStatus::Idle, None)).unwrap();
    assert_eq!(before.entries.len(), 2);

    trash_project(tmp.path(), &b.id).unwrap();

    let after = rebuild_with_status(&store, |_| (LibraryStatus::Idle, None)).unwrap();
    assert_eq!(after.entries.len(), 1, "trashed entry gone from index");
    assert_eq!(after.entries[0].id, a.id);
}
