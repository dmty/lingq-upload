//! Chapter-selection contract.
//!
//! Covers the per-project skipped-chapter vector across both stores plus the
//! orchestrator's "skipped chapters never reach LingQ" guarantee.

use lingq_upload_lib::core::audio::AbsorbPolicy;
use std::path::{Path, PathBuf};
use std::process::Command as SyncCommand;
use std::sync::{Arc, Mutex};

use mockito::{Matcher, Server, ServerGuard};
use secrecy::SecretString;
use tokio_util::sync::CancellationToken;

use lingq_upload_lib::core::epub::{Chapter, ChapterId, ChapterKind};
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::{run_project_job, JobSink};
use lingq_upload_lib::core::matcher::{BucketPreview, MismatchCondition, MismatchResponse};
use lingq_upload_lib::core::project::{
    ChapterReceipt, Project, ProjectSettings, ProjectSources, SCHEMA_V1,
};
use lingq_upload_lib::core::store::{
    InMemoryProjectStore, JsonProjectStore, ProjectStore, StoreError,
};
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use lingq_upload_lib::lingq::{LanguageCode, LingqClient};
use tempfile::TempDir;

// --- Sample data ------------------------------------------------------------

fn sample(title: &str, n_receipts: usize) -> Project {
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
        receipts: (0..n_receipts)
            .map(|i| ChapterReceipt {
                chapter_index: i,
                track_index: Some(i),
                lesson_id: None,
                degraded: false,
                uploaded_at: None,
            })
            .collect(),
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
        absorb_policy: AbsorbPolicy::default(),
        mapping: None,
    }
}

// --- Parameterised store contract -------------------------------------------

fn cid(order: usize) -> ChapterId {
    ChapterId::from_order(order)
}

#[test]
fn set_selection_round_trips_across_stores() {
    let tmp = TempDir::new().unwrap();
    let json_store: Box<dyn ProjectStore> = Box::new(JsonProjectStore::new(tmp.path()));
    let mem_store: Box<dyn ProjectStore> = Box::new(InMemoryProjectStore::new());

    for store in [json_store, mem_store] {
        let mut p = sample("Selection RT", 4);
        store.put(&p).unwrap();

        // Empty → some ids.
        store.set_selection(&p.id, &[cid(2), cid(0)]).unwrap();
        let got = store.get(&p.id).unwrap().unwrap();
        assert_eq!(got.skipped_chapters, vec![cid(0), cid(2)], "sorted + deduped");

        // Replace wholesale: ids absent from the new set must clear.
        store.set_selection(&p.id, &[cid(1)]).unwrap();
        let got = store.get(&p.id).unwrap().unwrap();
        assert_eq!(got.skipped_chapters, vec![cid(1)], "wholesale replace");

        // Dedup of duplicates.
        store
            .set_selection(&p.id, &[cid(3), cid(3), cid(1), cid(1), cid(3)])
            .unwrap();
        let got = store.get(&p.id).unwrap().unwrap();
        assert_eq!(got.skipped_chapters, vec![cid(1), cid(3)]);

        // Clear.
        store.set_selection(&p.id, &[]).unwrap();
        let got = store.get(&p.id).unwrap().unwrap();
        assert!(got.skipped_chapters.is_empty());

        // Other fields untouched.
        p.skipped_chapters = vec![];
        assert_eq!(got, p);

        // NotFound on unknown id.
        let ghost = ProjectId::from_title_author("ghost", "nobody");
        match store.set_selection(&ghost, &[cid(0)]) {
            Err(StoreError::NotFound { .. }) => (),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}

#[test]
fn chapter_kind_default_is_body() {
    let c = Chapter::default();
    assert_eq!(c.kind, ChapterKind::Body);
    assert_eq!(c.id, ChapterId::default());
}

#[test]
fn chapter_kind_round_trips_through_json() {
    let mut c = Chapter {
        order: 7,
        title: "Preface".into(),
        body: "x".into(),
        id: cid(7),
        kind: ChapterKind::FrontMatter,
    };
    let s = serde_json::to_string(&c).unwrap();
    let back: Chapter = serde_json::from_str(&s).unwrap();
    assert_eq!(back, c);

    // Default fields elided in older JSON still parse: `id` and `kind`
    // both default; chapters from an older save get the empty placeholder
    // id until the next parse re-stamps it.
    let bare = r#"{"order":0,"title":"t","body":"b"}"#;
    let p: Chapter = serde_json::from_str(bare).unwrap();
    assert_eq!(p.id, ChapterId::default());
    assert_eq!(p.kind, ChapterKind::Body);

    c.kind = ChapterKind::BackMatter;
    let s = serde_json::to_string(&c).unwrap();
    assert!(s.contains("back_matter"), "snake_case rename: {s}");
}

/// Baseline shape from before this refactor: no `Chapter.id` field, no
/// `skipped_chapters` on the project. Must still deserialise; new fields
/// fall back to their defaults so legacy saves keep loading.
#[test]
fn baseline_project_json_loads_with_default_selection_and_ids() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/projects/baseline_no_selection.json");
    let bytes = std::fs::read(&path).expect("read fixture");
    let project: Project = serde_json::from_slice(&bytes).expect("parse baseline project.json");

    assert!(
        project.skipped_chapters.is_empty(),
        "missing skipped_chapters must default to empty"
    );

    // The fixture also includes a serialised chapter list in
    // `_chapters_for_migration_check` so we can assert the default id
    // without touching the runtime Project schema.
    let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    if let Some(chs) = raw.get("_chapters_for_migration_check") {
        let chapters: Vec<Chapter> = serde_json::from_value(chs.clone()).unwrap();
        assert!(!chapters.is_empty());
        for c in &chapters {
            assert_eq!(
                c.id,
                ChapterId::default(),
                "legacy chapter must default to ChapterId::default()"
            );
        }
    }
}

// --- Orchestrator gate ------------------------------------------------------

fn which(bin: &str) -> Option<PathBuf> {
    SyncCommand::new("which")
        .arg(bin)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
}

fn ffmpeg_available() -> bool {
    which("ffmpeg").is_some() && which("ffprobe").is_some()
}

#[derive(Default, Clone)]
struct RecordingSink {
    chapter_dones: Arc<Mutex<Vec<usize>>>,
    result_ok: Arc<Mutex<Option<bool>>>,
}

impl JobSink for RecordingSink {
    fn started(&mut self) {}
    fn progress(&mut self, _pct: f32, _message: Option<String>) {}
    fn chapter_done(&mut self, chapter_index: usize, _lesson_id: i64, _degraded: bool) {
        self.chapter_dones.lock().unwrap().push(chapter_index);
    }
    fn cancelled(&mut self) {}
    fn result(&mut self, ok: bool, _payload: serde_json::Value) {
        *self.result_ok.lock().unwrap() = Some(ok);
    }
    fn needs_match(
        &mut self,
        _title: String,
        _chapters: usize,
        _tracks: usize,
        _condition: MismatchCondition,
        _options: Vec<MismatchResponse>,
        _preselect: MismatchResponse,
        _bucket_preview: Option<Vec<BucketPreview>>,
    ) {
    }
}

struct Fixture {
    server: ServerGuard,
    store: Arc<dyn ProjectStore>,
    project_id: ProjectId,
    _audio_dir: tempfile::TempDir,
}

fn make_chapter_files(dir: &Path, count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|i| {
            let p = dir.join(format!("ch_{:02}.txt", i + 1));
            std::fs::write(&p, format!("Body of chapter {}.\n", i + 1)).unwrap();
            p
        })
        .collect()
}

async fn make_fixture(chapters: usize) -> Fixture {
    let server = Server::new_async().await;
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());

    let audio_dir = tempfile::tempdir().unwrap();
    let probe =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/probe_3min.mp3");
    for i in 0..chapters {
        let dst = audio_dir.path().join(format!("track_{:02}.mp3", i + 1));
        std::fs::copy(&probe, &dst).unwrap();
    }

    let text_dir = tempfile::tempdir().unwrap();
    let text_paths = make_chapter_files(text_dir.path(), chapters);
    std::mem::forget(text_dir);

    let id = ProjectId::from_title_author("Sel Book", "Author");
    let project = Project {
        schema_version: SCHEMA_V1,
        id: id.clone(),
        sources: ProjectSources {
            text: TextSource::LooseFiles { paths: text_paths },
            audio: Some(AudioSource::Folder(audio_dir.path().to_path_buf())),
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "ja".into(),
            collection_title: "Sel Book".into(),
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
        absorb_policy: AbsorbPolicy::default(),
        mapping: None,
    };
    store.put(&project).unwrap();

    Fixture {
        server,
        store,
        project_id: id,
        _audio_dir: audio_dir,
    }
}

fn mock_collection(server: &mut ServerGuard, collection_id: i64) {
    let _ = server
        .mock(
            "GET",
            Matcher::Regex(r"^/api/v3/ja/collections/\?search=".into()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(
            r#"{{"results":[{{"pk":{collection_id},"title":"Sel Book"}}]}}"#
        ))
        .create();
}

#[tokio::test]
async fn run_skips_marked_chapters_and_imports_only_remainder() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping selection upload gate");
        return;
    }
    let total = 4usize;
    let skipped_idx = [1usize, 3];
    let skipped_ids: Vec<ChapterId> = skipped_idx.iter().map(|i| cid(*i)).collect();
    let expected_imports = total - skipped_idx.len();

    let mut fixture = make_fixture(total).await;
    mock_collection(&mut fixture.server, 4242);
    fixture
        .store
        .set_selection(&fixture.project_id, &skipped_ids)
        .unwrap();

    // mockito has no global counter — one mock per expected call, each
    // pinned to a single hit. `expect(1)` causes mock.assert() to panic if
    // the mock is hit a different number of times (i.e. zero hits if the
    // orchestrator's gate fails open).
    let mocks: Vec<_> = (0..expected_imports)
        .map(|i| {
            let id = 5000 + i as i64;
            fixture
                .server
                .mock("POST", "/api/v3/ja/lessons/import/")
                .with_status(201)
                .with_header("content-type", "application/json")
                .with_body(format!(r#"{{"pk":{id}}}"#))
                .expect(1)
                .create()
        })
        .collect();

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        LanguageCode::new("ja").unwrap(),
        fixture.server.url(),
    ));
    let mut sink = RecordingSink::default();
    run_project_job(
        fixture.store.clone(),
        client,
        fixture.project_id.clone(),
        CancellationToken::new(),
        &mut sink,
    )
    .await
    .expect("orchestrator run");

    // mockito asserts that each mock was hit exactly once. The sum
    // is precisely `total - skipped.len()` imports.
    for m in &mocks {
        m.assert();
    }

    let dones = sink.chapter_dones.lock().unwrap().clone();
    assert_eq!(dones.len(), expected_imports, "got dones {dones:?}");
    // Skipped indices must never appear in chapter_done events.
    for s in &skipped_idx {
        assert!(!dones.contains(s), "skipped chapter leaked: {dones:?}");
    }

    let project = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    let uploaded: Vec<_> = project
        .receipts
        .iter()
        .filter_map(|r| r.lesson_id.map(|_| r.chapter_index))
        .collect();
    assert_eq!(uploaded.len(), expected_imports);
    for s in &skipped_idx {
        assert!(!uploaded.contains(s), "skipped chapter has receipt: {uploaded:?}");
    }
}

#[tokio::test]
async fn skipping_after_upload_does_not_delete_existing_lesson() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping post-upload-skip test");
        return;
    }
    let total = 3usize;
    let mut fixture = make_fixture(total).await;
    mock_collection(&mut fixture.server, 9999);

    // First run uploads all three.
    let first_mocks: Vec<_> = (0..total)
        .map(|i| {
            let id = 7000 + i as i64;
            fixture
                .server
                .mock("POST", "/api/v3/ja/lessons/import/")
                .with_status(201)
                .with_header("content-type", "application/json")
                .with_body(format!(r#"{{"pk":{id}}}"#))
                .expect(1)
                .create()
        })
        .collect();

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        LanguageCode::new("ja").unwrap(),
        fixture.server.url(),
    ));
    let mut sink = RecordingSink::default();
    run_project_job(
        fixture.store.clone(),
        client.clone(),
        fixture.project_id.clone(),
        CancellationToken::new(),
        &mut sink,
    )
    .await
    .expect("first run");
    for m in &first_mocks {
        m.assert();
    }

    // Capture lesson_ids of all three chapters before mutating the
    // selection.
    let before = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    let lessons_before: Vec<(usize, i64)> = before
        .receipts
        .iter()
        .filter_map(|r| r.lesson_id.map(|id| (r.chapter_index, id)))
        .collect();
    assert_eq!(lessons_before.len(), total);

    // Now skip a previously-uploaded chapter and re-run.
    //
    // Stronger than `expect(0)` on a never-fired DELETE route: pin an
    // *upload* mock with `expect(0)` so the assertion fires only if the
    // orchestrator decides to re-import (the only way LingQ traffic could
    // touch chapter A again, since the client has no DELETE endpoint).
    fixture
        .store
        .set_selection(&fixture.project_id, &[cid(1)])
        .unwrap();
    let upload_quiet = fixture
        .server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(201)
        .with_body(r#"{"pk":99999}"#)
        .expect(0)
        .create();

    let chapter_done_count_before = sink.chapter_dones.lock().unwrap().len();
    let mut sink2 = RecordingSink::default();
    run_project_job(
        fixture.store.clone(),
        client,
        fixture.project_id.clone(),
        CancellationToken::new(),
        &mut sink2,
    )
    .await
    .expect("second run");

    // No further upload traffic for any chapter — all three already carry
    // lesson_ids; chapter 1 is also in the skip set.
    upload_quiet.assert();
    assert!(
        sink2.chapter_dones.lock().unwrap().is_empty(),
        "no new chapter_done events on resume; pre-run had {chapter_done_count_before}",
    );

    // Each chapter's original lesson_id is preserved verbatim.
    let after = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    for (idx, id) in &lessons_before {
        let still = after
            .receipts
            .iter()
            .find(|r| r.chapter_index == *idx)
            .and_then(|r| r.lesson_id);
        assert_eq!(
            still,
            Some(*id),
            "chapter {idx} lesson_id changed after post-upload skip",
        );
    }
    assert_eq!(after.skipped_chapters, vec![cid(1)]);
}
