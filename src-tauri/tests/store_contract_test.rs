use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::{
    ChapterReceipt, Project, ProjectSettings, ProjectSources, ProjectStage, SCHEMA_V1,
};
use lingq_upload_lib::core::store::{
    safe_path_segment, InMemoryProjectStore, JsonProjectStore, ListHealth, ProjectStore, StoreError,
};
use lingq_upload_lib::ingest::TextSource;
use tempfile::TempDir;

fn sample(title: &str, language: &str) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id: ProjectId::from_title_author(title, "村上春樹"),
        sources: ProjectSources {
            text: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: language.into(),
            collection_title: title.into(),
            level: 1,
            tags: vec![],
        },
        receipts: vec![ChapterReceipt {
            chapter_index: 0,
            track_index: Some(0),
            lesson_id: Some(42),
            degraded: false,
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
        stage: ProjectStage::New,
        last_transition_at: None,
    skipped_chapters: vec![],
    }
}

fn sample_with_receipts(title: &str, language: &str, n: usize) -> Project {
    let mut p = sample(title, language);
    p.receipts = (0..n)
        .map(|i| ChapterReceipt {
            chapter_index: i,
            track_index: Some(i),
            lesson_id: Some(1000 + i as i64),
            degraded: false,
            uploaded_at: Some(Utc::now()),
        })
        .collect();
    p
}

fn run_contract(store: &dyn ProjectStore) {
    let p = sample("Foo Book", "ja");

    store.put(&p).unwrap();
    let got = store.get(&p.id).unwrap().unwrap();
    assert_eq!(got, p, "put → get round-trip");

    let list = store.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].title, "Foo Book");

    let p2 = sample("Another Book", "ja");
    store.put(&p2).unwrap();
    let list = store.list().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].title, "Another Book", "sorted by title");

    let unknown = ProjectId::from_title_author("nope", "nobody");
    assert!(store.get(&unknown).unwrap().is_none());

    patch_chapter_updates_indexed_receipt_only(store);
    patch_chapter_out_of_bounds_returns_error(store);
    patch_chapter_not_found_returns_error(store);
}

fn patch_chapter_updates_indexed_receipt_only(store: &dyn ProjectStore) {
    let p = sample_with_receipts("Patch Target", "ja", 3);
    store.put(&p).unwrap();

    let new_receipt = ChapterReceipt {
        chapter_index: 1,
        track_index: Some(99),
        lesson_id: Some(7777),
        degraded: true,
        uploaded_at: Some(Utc::now()),
    };
    store.patch_chapter(&p.id, 1, new_receipt.clone()).unwrap();

    let after = store.get(&p.id).unwrap().unwrap();
    assert_eq!(after.receipts.len(), 3);
    assert_eq!(after.receipts[0], p.receipts[0], "index 0 untouched");
    assert_eq!(after.receipts[1], new_receipt, "index 1 replaced");
    assert_eq!(after.receipts[2], p.receipts[2], "index 2 untouched");

    let mut expected = p.clone();
    expected.receipts[1] = new_receipt;
    assert_eq!(after, expected, "all other Project fields preserved");
}

fn patch_chapter_out_of_bounds_returns_error(store: &dyn ProjectStore) {
    let p = sample_with_receipts("Bounds Target", "ja", 1);
    store.put(&p).unwrap();
    let before = store.get(&p.id).unwrap().unwrap();

    let r = ChapterReceipt {
        chapter_index: 5,
        track_index: None,
        lesson_id: None,
        degraded: false,
        uploaded_at: None,
    };
    match store.patch_chapter(&p.id, 5, r) {
        Err(StoreError::OutOfBounds { index, len }) => {
            assert_eq!(index, 5);
            assert_eq!(len, 1);
        }
        other => panic!("expected OutOfBounds, got {other:?}"),
    }

    let after = store.get(&p.id).unwrap().unwrap();
    assert_eq!(after, before, "no partial write on OOB");
}

fn patch_chapter_not_found_returns_error(store: &dyn ProjectStore) {
    let missing = ProjectId::from_title_author("never-put", "ghost");
    let r = ChapterReceipt {
        chapter_index: 0,
        track_index: None,
        lesson_id: None,
        degraded: false,
        uploaded_at: None,
    };
    match store.patch_chapter(&missing, 0, r) {
        Err(StoreError::NotFound { .. }) => (),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn json_store_passes_contract() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    run_contract(&store);
}

#[test]
fn in_memory_store_passes_contract() {
    let store = InMemoryProjectStore::new();
    run_contract(&store);
}

#[test]
fn trait_is_object_safe() {
    let arc: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());
    let p = sample("X", "ja");
    arc.put(&p).unwrap();
    assert!(arc.get(&p.id).unwrap().is_some());
}

#[test]
fn empty_project_json_deserialises_via_defaults() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("Minimal", "Author");
    let key = safe_path_segment(&id.join_key());
    let dir = tmp.path().join("projects").join(&key);
    std::fs::create_dir_all(&dir).unwrap();

    let minimal = serde_json::json!({
        "id": id,
        "sources": {
            "text": { "kind": "epub", "value": "/tmp/x.epub" }
        },
        "settings": {
            "language": "ja",
            "collection_title": "Minimal"
        }
    });
    std::fs::write(dir.join("project.json"), minimal.to_string()).unwrap();

    let p = store.get(&id).unwrap().expect("present");
    assert_eq!(p.schema_version, SCHEMA_V1);
    assert!(p.receipts.is_empty());
    assert_eq!(p.queue_cursor, 0);
    assert!(p.matcher_decision.is_none());
    assert_eq!(p.settings.level, 1);
}

#[test]
fn corrupt_json_returns_corrupt_error() {
    use lingq_upload_lib::core::store::StoreError;
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("Bad", "Author");
    let dir = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&id.join_key()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("project.json"), b"{ not json }").unwrap();

    match store.get(&id) {
        Err(StoreError::Corrupt { .. }) => (),
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

#[test]
fn atomic_write_leaves_no_tmp_files_behind() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let p = sample("Foo", "ja");
    store.put(&p).unwrap();

    let dir = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&p.id.join_key()));
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        entries.iter().any(|n| n == "project.json"),
        "project.json present"
    );
    assert!(
        !entries
            .iter()
            .any(|n| n.to_string_lossy().ends_with(".tmp")),
        "no .tmp files: {entries:?}"
    );
}

#[test]
fn powercut_simulation_keeps_prior_file() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let p1 = sample("Foo", "ja");
    store.put(&p1).unwrap();

    let pj = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&p1.id.join_key()))
        .join("project.json");
    let tmp_path = pj.with_extension("json.tmp");
    std::fs::write(&tmp_path, b"{ partial write before rename }").unwrap();

    let got = store.get(&p1.id).unwrap().unwrap();
    assert_eq!(got, p1, "prior file untouched");
}

#[test]
fn powercut_simulation_preserves_prior_selection() {
    // `set_selection` shares the tempfile + fsync + rename path with `put`.
    // A partial write that never gets renamed must not leak into the
    // visible project.
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let mut p = sample_with_receipts("Selection PowerCut", "ja", 3);
    p.skipped_chapters = vec![ChapterId::from_order(0)];
    store.put(&p).unwrap();

    let pj = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&p.id.join_key()))
        .join("project.json");
    let tmp_path = pj.with_extension("json.tmp");
    std::fs::write(&tmp_path, b"{ partial selection write }").unwrap();

    let got = store.get(&p.id).unwrap().unwrap();
    assert_eq!(got.skipped_chapters, vec![ChapterId::from_order(0)]);
    assert_eq!(got, p, "prior file untouched after partial tmp write");

    // Real selection update must also leave no tmp turds.
    store
        .set_selection(&p.id, &[ChapterId::from_order(1), ChapterId::from_order(2)])
        .unwrap();
    let dir = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&p.id.join_key()));
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        !entries
            .iter()
            .any(|n| n.to_string_lossy().ends_with(".tmp")),
        "no .tmp files after set_selection: {entries:?}"
    );
    let got = store.get(&p.id).unwrap().unwrap();
    assert_eq!(
        got.skipped_chapters,
        vec![ChapterId::from_order(1), ChapterId::from_order(2)]
    );
}

#[test]
fn list_skips_corrupt_entries_and_returns_good_ones() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());

    let good = sample("Good Book", "ja");
    store.put(&good).unwrap();

    let bad_id = ProjectId::from_title_author("Bad Book", "Author");
    let bad_dir = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&bad_id.join_key()));
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join("project.json"), b"{ not json }").unwrap();

    let list = store.list().unwrap();
    assert_eq!(list.len(), 1, "corrupt entry skipped, good entry returned");
    assert_eq!(list[0].title, "Good Book");
}

#[test]
fn list_dedupes_when_same_id_exists_under_two_directories() {
    // Path sanitisation (`:` -> `_`) can leave the same logical project
    // under two on-disk dirs after upgrade. `list` must collapse those.
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());

    let p = sample("Dupe Book", "ja");
    store.put(&p).unwrap();

    // Manually plant a second directory whose name pretends to be the
    // legacy unsanitised key, containing the same project bytes.
    let legacy_dir = tmp
        .path()
        .join("projects")
        .join(format!("legacy_{}", safe_path_segment(&p.id.join_key())));
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let bytes = serde_json::to_vec_pretty(&p).unwrap();
    std::fs::write(legacy_dir.join("project.json"), bytes).unwrap();

    let list = store.list().unwrap();
    assert_eq!(list.len(), 1, "duplicate id collapses to a single entry");
}

#[test]
fn health_reports_ok_corrupt_and_deduped_counts() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());

    let good = sample("Health Good", "ja");
    store.put(&good).unwrap();

    let bad_id = ProjectId::from_title_author("Health Bad", "Author");
    let bad_dir = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&bad_id.join_key()));
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join("project.json"), b"{ not json }").unwrap();

    let dup = sample("Health Dupe", "ja");
    store.put(&dup).unwrap();
    let legacy_dir = tmp
        .path()
        .join("projects")
        .join(format!("legacy_{}", safe_path_segment(&dup.id.join_key())));
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let bytes = serde_json::to_vec_pretty(&dup).unwrap();
    std::fs::write(legacy_dir.join("project.json"), bytes).unwrap();

    let ListHealth {
        ok,
        corrupt,
        deduped,
    } = store.health().unwrap();
    assert_eq!(ok, 2, "two distinct good ids");
    assert_eq!(corrupt.len(), 1, "one corrupt file surfaced");
    assert_eq!(deduped.len(), 1, "one duplicate suppressed");

    let list = store.list().unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn list_dedupe_winner_is_most_recently_modified() {
    use std::thread::sleep;
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());

    let mut older = sample("Order Probe", "ja");
    older.settings.collection_title = "OLD".into();
    store.put(&older).unwrap();

    let legacy_dir = tmp.path().join("projects").join(format!(
        "legacy_{}",
        safe_path_segment(&older.id.join_key())
    ));
    std::fs::create_dir_all(&legacy_dir).unwrap();
    sleep(Duration::from_millis(50));
    let mut newer = older.clone();
    newer.settings.collection_title = "NEW".into();
    std::fs::write(
        legacy_dir.join("project.json"),
        serde_json::to_vec_pretty(&newer).unwrap(),
    )
    .unwrap();

    let list = store.list().unwrap();
    assert_eq!(list.len(), 1, "duplicate id collapses");
    assert_eq!(list[0].title, "NEW", "most-recently-modified wins");
}

#[test]
fn json_store_patch_chapter_atomic() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let p = sample_with_receipts("Atomic Patch", "ja", 2);
    store.put(&p).unwrap();

    let new_receipt = ChapterReceipt {
        chapter_index: 0,
        track_index: Some(0),
        lesson_id: Some(424242),
        degraded: false,
        uploaded_at: Some(Utc::now()),
    };
    store.patch_chapter(&p.id, 0, new_receipt).unwrap();

    let dir = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&p.id.join_key()));
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        !entries
            .iter()
            .any(|n| n.to_string_lossy().ends_with(".tmp")),
        "no .tmp files: {entries:?}"
    );
}

#[test]
fn put_and_get_round_trip_strong_key_with_colons() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let mut p = sample("ASIN Book", "ja");
    p.id = p.id.with_asin("B0CROSS01");
    store.put(&p).unwrap();
    let got = store.get(&p.id).unwrap().unwrap();
    assert_eq!(got, p);
}
