//! Chapter-selection contract.
//!
//! Covers the per-project skipped-chapter vector across both stores plus the
//! orchestrator's "skipped chapters never reach LingQ" guarantee.

use std::path::{Path, PathBuf};
use std::process::Command as SyncCommand;
use std::sync::{Arc, Mutex};

use mockito::{Matcher, Server, ServerGuard};
use secrecy::SecretString;
use tokio_util::sync::CancellationToken;

use lingq_upload_lib::core::epub::{Chapter, ChapterKind};
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
use lingq_upload_lib::lingq::LingqClient;
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
    }
}

// --- Parameterised store contract -------------------------------------------

#[test]
fn set_selection_round_trips_across_stores() {
    let tmp = TempDir::new().unwrap();
    let json_store: Box<dyn ProjectStore> = Box::new(JsonProjectStore::new(tmp.path()));
    let mem_store: Box<dyn ProjectStore> = Box::new(InMemoryProjectStore::new());

    for store in [json_store, mem_store] {
        let mut p = sample("Selection RT", 4);
        store.put(&p).unwrap();

        // Empty → some ids.
        store.set_selection(&p.id, &[2, 0]).unwrap();
        let got = store.get(&p.id).unwrap().unwrap();
        assert_eq!(got.skipped_chapters, vec![0, 2], "sorted + deduped");

        // Replace wholesale: ids absent from the new set must clear.
        store.set_selection(&p.id, &[1]).unwrap();
        let got = store.get(&p.id).unwrap().unwrap();
        assert_eq!(got.skipped_chapters, vec![1], "wholesale replace");

        // Dedup of duplicates.
        store.set_selection(&p.id, &[3, 3, 1, 1, 3]).unwrap();
        let got = store.get(&p.id).unwrap().unwrap();
        assert_eq!(got.skipped_chapters, vec![1, 3]);

        // Clear.
        store.set_selection(&p.id, &[]).unwrap();
        let got = store.get(&p.id).unwrap().unwrap();
        assert!(got.skipped_chapters.is_empty());

        // Other fields untouched.
        p.skipped_chapters = vec![];
        assert_eq!(got, p);

        // NotFound on unknown id.
        let ghost = ProjectId::from_title_author("ghost", "nobody");
        match store.set_selection(&ghost, &[0]) {
            Err(StoreError::NotFound { .. }) => (),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}

#[test]
fn chapter_kind_default_is_body() {
    let c = Chapter::default();
    assert_eq!(c.kind, ChapterKind::Body);
    assert!(!c.skipped);
}

#[test]
fn chapter_kind_round_trips_through_json() {
    let mut c = Chapter {
        order: 7,
        title: "Preface".into(),
        body: "x".into(),
        skipped: true,
        kind: ChapterKind::FrontMatter,
    };
    let s = serde_json::to_string(&c).unwrap();
    let back: Chapter = serde_json::from_str(&s).unwrap();
    assert_eq!(back, c);

    // Default fields elided in older JSON still parse.
    let bare = r#"{"order":0,"title":"t","body":"b"}"#;
    let p: Chapter = serde_json::from_str(bare).unwrap();
    assert!(!p.skipped);
    assert_eq!(p.kind, ChapterKind::Body);

    c.kind = ChapterKind::BackMatter;
    let s = serde_json::to_string(&c).unwrap();
    assert!(s.contains("back_matter"), "snake_case rename: {s}");
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
    let skipped = [1usize, 3];
    let expected_imports = total - skipped.len();

    let mut fixture = make_fixture(total).await;
    mock_collection(&mut fixture.server, 4242);
    fixture
        .store
        .set_selection(&fixture.project_id, &skipped)
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
        "ja",
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
    for s in &skipped {
        assert!(!dones.contains(s), "skipped chapter leaked: {dones:?}");
    }

    let project = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    let uploaded: Vec<_> = project
        .receipts
        .iter()
        .filter_map(|r| r.lesson_id.map(|_| r.chapter_index))
        .collect();
    assert_eq!(uploaded.len(), expected_imports);
    for s in &skipped {
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
        "ja",
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

    // Now skip a previously-uploaded chapter and re-run. No DELETE must hit
    // the server — there is no DELETE mock installed, and mockito returns
    // 501 for unmatched routes. We also pin an upload mock with `expect(0)`
    // so the assertion fires if the orchestrator decides to re-import.
    fixture
        .store
        .set_selection(&fixture.project_id, &[1])
        .unwrap();
    let upload_quiet = fixture
        .server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(201)
        .with_body(r#"{"pk":99999}"#)
        .expect(0)
        .create();
    let delete_quiet = fixture
        .server
        .mock(
            "DELETE",
            Matcher::Regex(r"^/api/v3/ja/lessons/".into()),
        )
        .with_status(204)
        .expect(0)
        .create();

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

    upload_quiet.assert();
    delete_quiet.assert();

    // The lesson for chapter 1 is still on disk with its original id.
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
    assert_eq!(after.skipped_chapters, vec![1]);
}
