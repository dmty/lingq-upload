//! End-to-end orchestrator tests.
//!
//! These tests skip when `ffmpeg`/`ffprobe` are missing from PATH so devs
//! without ffmpeg installed don't see red — same convention as
//! `audio_golden.rs`.

use std::path::{Path, PathBuf};
use std::process::Command as SyncCommand;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use mockito::{Matcher, Server, ServerGuard};
use secrecy::SecretString;
use tokio_util::sync::CancellationToken;

use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::{run_project_job, JobSink};
use lingq_upload_lib::core::matcher::{BucketPreview, MismatchCondition, MismatchResponse};
use lingq_upload_lib::core::project::{
    ChapterReceipt, MatcherDecision, Project, ProjectSettings, ProjectSources, SCHEMA_V1,
};
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use lingq_upload_lib::lingq::LingqClient;

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
    events: Arc<Mutex<Vec<RecordedEvent>>>,
}

#[derive(Debug, Clone, PartialEq)]
enum RecordedEvent {
    Started,
    Progress(f32),
    ChapterDone {
        chapter_index: usize,
        lesson_id: i64,
        degraded: bool,
    },
    Cancelled,
    Result(bool),
    NeedsMatch,
}

impl JobSink for RecordingSink {
    fn started(&mut self) {
        self.events.lock().unwrap().push(RecordedEvent::Started);
    }
    fn progress(&mut self, pct: f32, _message: Option<String>) {
        self.events
            .lock()
            .unwrap()
            .push(RecordedEvent::Progress(pct));
    }
    fn chapter_done(&mut self, chapter_index: usize, lesson_id: i64, degraded: bool) {
        self.events
            .lock()
            .unwrap()
            .push(RecordedEvent::ChapterDone {
                chapter_index,
                lesson_id,
                degraded,
            });
    }
    fn cancelled(&mut self) {
        self.events.lock().unwrap().push(RecordedEvent::Cancelled);
    }
    fn result(&mut self, ok: bool, _payload: serde_json::Value) {
        self.events.lock().unwrap().push(RecordedEvent::Result(ok));
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
        self.events.lock().unwrap().push(RecordedEvent::NeedsMatch);
    }
}

struct Fixture {
    server: ServerGuard,
    store: Arc<dyn ProjectStore>,
    project_id: ProjectId,
    audio_dir: tempfile::TempDir,
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
    make_fixture_with_counts(chapters, chapters).await
}

async fn make_fixture_with_counts(chapters: usize, tracks: usize) -> Fixture {
    let server = Server::new_async().await;
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());

    // Stage `tracks` copies of the probe fixture into a fresh audio folder.
    let audio_dir = tempfile::tempdir().unwrap();
    let probe =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/probe_3min.mp3");
    for i in 0..tracks {
        let dst = audio_dir.path().join(format!("track_{:02}.mp3", i + 1));
        std::fs::copy(&probe, &dst).unwrap();
    }

    // Loose-files text source — one .txt per chapter, sorted lexically.
    let text_dir = tempfile::tempdir().unwrap();
    let text_paths = make_chapter_files(text_dir.path(), chapters);
    // Leak the tempdir so the files survive — InMemoryProjectStore holds the path.
    std::mem::forget(text_dir);

    let id = ProjectId::from_title_author("My Book", "Author");
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
            collection_title: "My Book".into(),
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
    };
    store.put(&project).unwrap();

    Fixture {
        server,
        store,
        project_id: id,
        audio_dir,
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
            r#"{{"results":[{{"pk":{collection_id},"title":"My Book"}}]}}"#
        ))
        .create();
}

fn mock_imports(server: &mut ServerGuard, expected: usize, start_id: i64) {
    // Each successful import returns a fresh lesson id. mockito doesn't have
    // a counter, so we install one mock per expected call, each pinned to a
    // single hit. The exact order isn't enforced by mockito itself — but our
    // assertions check the receipt set, which captures all returned ids.
    for i in 0..expected {
        let id = start_id + i as i64;
        let _ = server
            .mock("POST", "/api/v3/ja/lessons/import/")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(format!(r#"{{"pk":{id}}}"#))
            .expect(1)
            .create();
    }
}

#[tokio::test]
async fn happy_path_three_chapters_three_tracks() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping happy_path_three_chapters_three_tracks");
        return;
    }
    let mut fixture = make_fixture(3).await;
    mock_collection(&mut fixture.server, 4242);
    mock_imports(&mut fixture.server, 3, 1000);

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        "ja",
        fixture.server.url(),
    ));
    let mut sink = RecordingSink::default();
    let token = CancellationToken::new();
    run_project_job(
        fixture.store.clone(),
        client,
        fixture.project_id.clone(),
        token,
        &mut sink,
    )
    .await
    .expect("orchestrator run");

    let events = sink.events.lock().unwrap().clone();
    let chapter_dones: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RecordedEvent::ChapterDone {
                chapter_index,
                lesson_id,
                ..
            } => Some((*chapter_index, *lesson_id)),
            _ => None,
        })
        .collect();
    assert_eq!(chapter_dones.len(), 3, "got events {:?}", events);
    assert_eq!(chapter_dones[0].0, 0);
    assert_eq!(chapter_dones[1].0, 1);
    assert_eq!(chapter_dones[2].0, 2);

    assert!(matches!(events.first(), Some(RecordedEvent::Started)));
    assert!(matches!(events.last(), Some(RecordedEvent::Result(true))));

    let project = fixture
        .store
        .get(&fixture.project_id)
        .unwrap()
        .expect("project persisted");
    assert_eq!(project.receipts.len(), 3);
    for (i, r) in project.receipts.iter().enumerate() {
        assert_eq!(r.chapter_index, i);
        assert!(r.lesson_id.is_some(), "chapter {i} missing lesson_id");
        assert!(r.uploaded_at.is_some());
    }
    assert_eq!(project.completed_lesson_ids.len(), 3);

    let _ = fixture.audio_dir; // keep alive
}

#[tokio::test]
async fn cancellation_after_first_chapter_stops_the_run() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping cancellation test");
        return;
    }
    let mut fixture = make_fixture(3).await;
    mock_collection(&mut fixture.server, 4242);
    // Only the first import should land — we cancel before the second.
    let _import = fixture
        .server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(201)
        .with_body(r#"{"pk":2001}"#)
        .expect_at_least(1)
        .create();

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        "ja",
        fixture.server.url(),
    ));
    let token = CancellationToken::new();
    let token_for_canceller = token.clone();

    // Cancel as soon as the first ChapterDone lands.
    let sink = RecordingSink::default();
    let events_handle = sink.events.clone();
    let canceller = tokio::spawn(async move {
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let evs = events_handle.lock().unwrap().clone();
            if evs
                .iter()
                .any(|e| matches!(e, RecordedEvent::ChapterDone { .. }))
            {
                token_for_canceller.cancel();
                return;
            }
        }
    });

    let mut sink_for_run = sink.clone();
    run_project_job(
        fixture.store.clone(),
        client,
        fixture.project_id.clone(),
        token,
        &mut sink_for_run,
    )
    .await
    .expect("orchestrator run");
    canceller.await.unwrap();

    let events = sink.events.lock().unwrap().clone();
    let done_count = events
        .iter()
        .filter(|e| matches!(e, RecordedEvent::ChapterDone { .. }))
        .count();
    assert!(done_count >= 1, "got events {:?}", events);
    assert!(
        events.iter().any(|e| matches!(e, RecordedEvent::Cancelled)),
        "expected Cancelled, got {:?}",
        events,
    );

    let project = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    assert!(
        project.receipts.len() < 3,
        "expected partial receipts; got {}",
        project.receipts.len(),
    );

    let _ = fixture.audio_dir;
}

#[tokio::test]
async fn resume_skips_chapters_already_uploaded() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping resume test");
        return;
    }
    let mut fixture = make_fixture(3).await;
    mock_collection(&mut fixture.server, 4242);
    // Only two imports — chapter 0 is already done.
    mock_imports(&mut fixture.server, 2, 2000);

    let mut project = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    project.receipts.push(ChapterReceipt {
        chapter_index: 0,
        track_index: Some(0),
        lesson_id: Some(999),
        degraded: false,
        uploaded_at: Some(Utc::now()),
    });
    project.queue_cursor = 1;
    project.completed_lesson_ids.push(999);
    fixture.store.put(&project).unwrap();

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

    let events = sink.events.lock().unwrap().clone();
    let done_indices: Vec<usize> = events
        .iter()
        .filter_map(|e| match e {
            RecordedEvent::ChapterDone { chapter_index, .. } => Some(*chapter_index),
            _ => None,
        })
        .collect();
    assert_eq!(done_indices, vec![1, 2], "got events {:?}", events);

    let project = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    assert_eq!(project.receipts.len(), 3);
    assert_eq!(project.receipts[0].lesson_id, Some(999));

    let _ = fixture.audio_dir;
}

/// PairAccept with more tracks than chapters: paired chapters use real
/// chapter text; leftover tracks ship as degraded audio-only lessons with
/// a single-space body. 2 chapters + 4 tracks → 4 import calls, 4 receipts;
/// receipts 0/1 carry chapter body, 2/3 are degraded with " ".
#[tokio::test]
async fn pair_accept_uploads_leftover_tracks_as_audio_only() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping pair_accept_leftover test");
        return;
    }
    let mut fixture = make_fixture_with_counts(2, 4).await;
    mock_collection(&mut fixture.server, 4242);
    mock_imports(&mut fixture.server, 4, 5000);

    // Record the PairAccept decision up front so the orchestrator skips the
    // NeedsMatch pause and runs the plan_from_decision path.
    let mut project = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    project.matcher_decision = Some(MatcherDecision {
        condition: MismatchCondition::CountOff,
        response: MismatchResponse::PairAccept,
        chapter_count: 2,
        track_count: 4,
        user_overrode: false,
        decided_at: Utc::now(),
    });
    fixture.store.put(&project).unwrap();

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

    let events = sink.events.lock().unwrap().clone();
    let done_count = events
        .iter()
        .filter(|e| matches!(e, RecordedEvent::ChapterDone { .. }))
        .count();
    assert_eq!(
        done_count, 4,
        "expected 4 ChapterDone events; got {:?}",
        events
    );
    assert!(matches!(events.last(), Some(RecordedEvent::Result(true))));

    let project = fixture.store.get(&fixture.project_id).unwrap().unwrap();
    assert_eq!(
        project.receipts.len(),
        4,
        "four receipts (2 paired + 2 leftover)"
    );

    // Paired receipts (chapter_index 0, 1) are not degraded.
    let paired: Vec<_> = project
        .receipts
        .iter()
        .filter(|r| r.chapter_index < 2)
        .collect();
    assert_eq!(paired.len(), 2);
    assert!(
        paired.iter().all(|r| !r.degraded),
        "paired receipts must not be degraded"
    );

    // Leftover receipts (chapter_index 2, 3 — synthetic = track index) ARE
    // degraded. The orchestrator emits `chapter_index: k` for leftovers so
    // resume-skip semantics work without colliding with paired chapters.
    let leftover: Vec<_> = project
        .receipts
        .iter()
        .filter(|r| r.chapter_index >= 2)
        .collect();
    assert_eq!(leftover.len(), 2, "got {:?}", project.receipts);
    assert!(
        leftover.iter().all(|r| r.degraded),
        "leftover receipts must be degraded"
    );

    let _ = fixture.audio_dir;
}
