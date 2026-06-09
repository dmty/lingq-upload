//! Per-lesson atomic flush + rehydration replay.
//!
//! Skips when `ffmpeg`/`ffprobe` are missing from PATH — same convention as
//! `job_orchestrator_test.rs`.

use std::path::{Path, PathBuf};
use std::process::Command as SyncCommand;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use mockito::{Matcher, Server, ServerGuard};
use secrecy::SecretString;
use tokio_util::sync::CancellationToken;

use lingq_upload_lib::commands::jobs::replay_receipts_impl;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::{run_project_job, JobSink};
use lingq_upload_lib::core::matcher::{BucketPreview, MismatchCondition, MismatchResponse};
use lingq_upload_lib::core::project::{
    ChapterReceipt, Project, ProjectSettings, ProjectSources, SCHEMA_V1,
};
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore, StoreError};
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use lingq_upload_lib::lingq::{LanguageCode, LingqClient};

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

// --- Recording store wrapper -------------------------------------------------

/// Wraps any `ProjectStore` and tallies `patch_chapter` invocations.
struct PatchSpy<S: ProjectStore> {
    inner: S,
    patch_calls: Mutex<Vec<(ProjectId, usize)>>,
}

impl<S: ProjectStore> PatchSpy<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            patch_calls: Mutex::new(Vec::new()),
        }
    }

    fn patch_count(&self) -> usize {
        self.patch_calls.lock().unwrap().len()
    }
}

impl<S: ProjectStore> ProjectStore for PatchSpy<S> {
    fn put(&self, p: &Project) -> Result<(), StoreError> {
        self.inner.put(p)
    }
    fn get(&self, id: &ProjectId) -> Result<Option<Project>, StoreError> {
        self.inner.get(id)
    }
    fn list(&self) -> Result<Vec<lingq_upload_lib::core::project::ProjectSummary>, StoreError> {
        self.inner.list()
    }
    fn patch_chapter(
        &self,
        id: &ProjectId,
        index: usize,
        receipt: ChapterReceipt,
    ) -> Result<(), StoreError> {
        self.patch_calls.lock().unwrap().push((id.clone(), index));
        self.inner.patch_chapter(id, index, receipt)
    }
    fn set_selection(
        &self,
        id: &ProjectId,
        skipped_ids: &[usize],
    ) -> Result<(), StoreError> {
        self.inner.set_selection(id, skipped_ids)
    }
}

// --- Sinks -------------------------------------------------------------------

#[derive(Default, Clone)]
struct RecordingSink {
    chapter_done_count: Arc<Mutex<usize>>,
    cancelled: Arc<Mutex<bool>>,
    result_ok: Arc<Mutex<Option<bool>>>,
}

impl JobSink for RecordingSink {
    fn started(&mut self) {}
    fn progress(&mut self, _pct: f32, _message: Option<String>) {}
    fn chapter_done(&mut self, _chapter_index: usize, _lesson_id: i64, _degraded: bool) {
        *self.chapter_done_count.lock().unwrap() += 1;
    }
    fn cancelled(&mut self) {
        *self.cancelled.lock().unwrap() = true;
    }
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

/// Sink that fires a cancellation token after the N-th `chapter_done`.
struct KillAfterNSink {
    inner: RecordingSink,
    kill_after: usize,
    token: CancellationToken,
}

impl JobSink for KillAfterNSink {
    fn started(&mut self) {
        self.inner.started();
    }
    fn progress(&mut self, pct: f32, message: Option<String>) {
        self.inner.progress(pct, message);
    }
    fn chapter_done(&mut self, chapter_index: usize, lesson_id: i64, degraded: bool) {
        self.inner.chapter_done(chapter_index, lesson_id, degraded);
        if *self.inner.chapter_done_count.lock().unwrap() >= self.kill_after {
            self.token.cancel();
        }
    }
    fn cancelled(&mut self) {
        self.inner.cancelled();
    }
    fn result(&mut self, ok: bool, payload: serde_json::Value) {
        self.inner.result(ok, payload);
    }
    fn needs_match(
        &mut self,
        title: String,
        chapters: usize,
        tracks: usize,
        condition: MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
        bucket_preview: Option<Vec<BucketPreview>>,
    ) {
        self.inner.needs_match(
            title,
            chapters,
            tracks,
            condition,
            options,
            preselect,
            bucket_preview,
        );
    }
}

// --- Fixture -----------------------------------------------------------------

fn make_chapter_files(dir: &Path, count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|i| {
            let p = dir.join(format!("ch_{:02}.txt", i + 1));
            std::fs::write(&p, format!("Body of chapter {}.\n", i + 1)).unwrap();
            p
        })
        .collect()
}

struct Fixture {
    server: ServerGuard,
    store: Arc<PatchSpy<InMemoryProjectStore>>,
    project_id: ProjectId,
    _audio_dir: tempfile::TempDir,
}

async fn make_fixture(chapters: usize) -> Fixture {
    let server = Server::new_async().await;
    let store = Arc::new(PatchSpy::new(InMemoryProjectStore::new()));

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

    let id = ProjectId::from_title_author("Resume Book", "Author");
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
            collection_title: "Resume Book".into(),
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
            r#"{{"results":[{{"pk":{collection_id},"title":"Resume Book"}}]}}"#
        ))
        .create();
}

/// Installs `count` import mocks, each pinned to exactly one hit. The mock
/// guards are returned so the caller can assert hit counts via `.assert()`.
fn mock_imports(server: &mut ServerGuard, count: usize, start_id: i64) -> Vec<mockito::Mock> {
    (0..count)
        .map(|i| {
            let id = start_id + i as i64;
            server
                .mock("POST", "/api/v3/ja/lessons/import/")
                .with_status(201)
                .with_header("content-type", "application/json")
                .with_body(format!(r#"{{"pk":{id}}}"#))
                .expect(1)
                .create()
        })
        .collect()
}

// --- Tests -------------------------------------------------------------------

/// AC2 + AC3: 5 chapters, cancel after 3 land, resume uploads only 4 and 5.
/// Combined POST count across both runs == 5; all 5 receipts carry a lesson_id.
#[tokio::test]
async fn five_chapters_killed_after_three_resumes_for_two() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping resume test");
        return;
    }
    let mut fixture = make_fixture(5).await;
    mock_collection(&mut fixture.server, 4242);
    // Five mocks total — three for run 1, two for run 2. mockito doesn't
    // distinguish runs; each mock is pinned to one hit.
    let imports = mock_imports(&mut fixture.server, 5, 7000);

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        LanguageCode::new("ja").expect("valid lang"),
        fixture.server.url(),
    ));

    // First run: cancel after the 3rd chapter_done.
    let token1 = CancellationToken::new();
    let mut sink1 = KillAfterNSink {
        inner: RecordingSink::default(),
        kill_after: 3,
        token: token1.clone(),
    };
    let store_dyn: Arc<dyn ProjectStore> = fixture.store.clone();
    run_project_job(
        store_dyn,
        client.clone(),
        fixture.project_id.clone(),
        token1,
        &mut sink1,
    )
    .await
    .expect("first orchestrator run");
    assert!(
        *sink1.inner.cancelled.lock().unwrap(),
        "first run cancelled"
    );
    assert_eq!(*sink1.inner.chapter_done_count.lock().unwrap(), 3);

    let mid = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    let mid_uploaded = mid
        .receipts
        .iter()
        .filter(|r| r.lesson_id.is_some())
        .count();
    assert_eq!(mid_uploaded, 3, "first run persisted 3 lesson_ids");
    assert_eq!(mid.receipts.len(), 5, "receipts pre-populated at Mapped");

    // Second run with a fresh cancel token — picks up where we left off.
    let token2 = CancellationToken::new();
    let mut sink2 = RecordingSink::default();
    let store_dyn2: Arc<dyn ProjectStore> = fixture.store.clone();
    run_project_job(
        store_dyn2,
        client,
        fixture.project_id.clone(),
        token2,
        &mut sink2,
    )
    .await
    .expect("second orchestrator run");
    assert_eq!(*sink2.result_ok.lock().unwrap(), Some(true));
    assert_eq!(
        *sink2.chapter_done_count.lock().unwrap(),
        2,
        "resume only uploads the remaining 2 chapters"
    );

    // Total POSTs across both runs must equal 5 — proves no re-upload.
    for m in &imports {
        m.assert();
    }

    let final_state = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    let final_uploaded = final_state
        .receipts
        .iter()
        .filter(|r| r.lesson_id.is_some())
        .count();
    assert_eq!(final_uploaded, 5, "final lesson count = 5");
}

/// AC1: `patch_chapter` is called once per uploaded chapter.
#[tokio::test]
async fn patch_chapter_call_count_matches_chapter_count() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping patch_chapter count test");
        return;
    }
    let mut fixture = make_fixture(3).await;
    mock_collection(&mut fixture.server, 4242);
    let _imports = mock_imports(&mut fixture.server, 3, 8000);

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        LanguageCode::new("ja").expect("valid lang"),
        fixture.server.url(),
    ));
    let mut sink = RecordingSink::default();
    let store_dyn: Arc<dyn ProjectStore> = fixture.store.clone();
    run_project_job(
        store_dyn,
        client,
        fixture.project_id.clone(),
        CancellationToken::new(),
        &mut sink,
    )
    .await
    .expect("orchestrator run");

    assert_eq!(*sink.result_ok.lock().unwrap(), Some(true));
    assert_eq!(
        fixture.store.patch_count(),
        3,
        "exactly one patch_chapter per uploaded chapter",
    );
}

/// AC4: rehydration replay returns persisted receipts in chapter order,
/// preserving lesson_id state (Some / None).
#[test]
fn cmd_replay_receipts_returns_persisted_state() {
    let store = InMemoryProjectStore::new();
    let id = ProjectId::from_title_author("Replay Book", "Author");
    let project = Project {
        schema_version: SCHEMA_V1,
        id: id.clone(),
        sources: ProjectSources {
            text: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "ja".into(),
            collection_title: "Replay Book".into(),
            level: 1,
            tags: vec![],
        },
        // Inserted out of chapter order to prove the impl sorts.
        receipts: vec![
            ChapterReceipt {
                chapter_index: 2,
                track_index: Some(2),
                lesson_id: None,
                degraded: false,
                uploaded_at: None,
            },
            ChapterReceipt {
                chapter_index: 0,
                track_index: Some(0),
                lesson_id: Some(11),
                degraded: false,
                uploaded_at: Some(Utc::now()),
            },
            ChapterReceipt {
                chapter_index: 1,
                track_index: Some(1),
                lesson_id: Some(22),
                degraded: true,
                uploaded_at: Some(Utc::now()),
            },
        ],
        queue_cursor: 2,
        completed_lesson_ids: vec![11, 22],
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

    let snaps = replay_receipts_impl(&store, &id).expect("replay receipts");
    assert_eq!(snaps.len(), 3);
    assert_eq!(snaps[0].chapter_index, 0);
    assert_eq!(snaps[0].lesson_id, Some(11));
    assert!(!snaps[0].degraded);
    assert_eq!(snaps[1].chapter_index, 1);
    assert_eq!(snaps[1].lesson_id, Some(22));
    assert!(snaps[1].degraded);
    assert_eq!(snaps[2].chapter_index, 2);
    assert_eq!(snaps[2].lesson_id, None);
    assert!(snaps[2].uploaded_at.is_none());
}

#[test]
fn cmd_replay_receipts_missing_project_errors() {
    let store = InMemoryProjectStore::new();
    let id = ProjectId::from_title_author("Nope", "Nobody");
    let err = replay_receipts_impl(&store, &id).unwrap_err();
    assert!(err.to_string().contains("project not found"), "{err}");
}
