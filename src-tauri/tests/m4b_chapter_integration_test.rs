//! End-to-end coverage for embedded m4b chapter atoms.
//!
//! Exercises the runtime contract in `docs/specs/m4b-chapters.md` (AD-023):
//! probe + atom fanout in `core::job::resolve_audio_tracks`, ManyToFew
//! classification + `bucket_preview` payload in the orchestrator's
//! NeedsMatch signal, and the windowed-transcode duration check in
//! `core::audio::transcode`.
//!
//! Manual smoke items NOT covered here:
//!   * LingQ collection contains the expected number of lessons after a
//!     real ManyToFew/SplitProportional run (requires the live LingQ API).
//!   * Drift indicator render + tooltip on the Mismatch UI's bucket
//!     preview rows (requires a browser).

mod support;

use lingq_upload_lib::core::audio::AbsorbPolicy;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use secrecy::SecretString;
use tokio_util::sync::CancellationToken;

use lingq_upload_lib::core::audio::{self, AudioError};
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::{run_project_job, JobSink};
use lingq_upload_lib::core::matcher::{BucketPreview, MismatchCondition, MismatchResponse};
use lingq_upload_lib::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use lingq_upload_lib::lingq::{LanguageCode, LingqClient};
use mockito::Server;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures/audio").join(name)
}

#[derive(Debug, Clone)]
struct NeedsMatchPayload {
    chapters: usize,
    tracks: usize,
    condition: MismatchCondition,
    options: Vec<MismatchResponse>,
    preselect: MismatchResponse,
    bucket_preview: Option<Vec<BucketPreview>>,
}

#[derive(Debug, Clone)]
enum Event {
    Started,
    ChapterDone { degraded: bool },
    Cancelled,
    Result(bool),
    NeedsMatch(NeedsMatchPayload),
    Progress,
}

#[derive(Default, Clone)]
struct RecordingSink {
    events: Arc<Mutex<Vec<Event>>>,
}

impl JobSink for RecordingSink {
    fn started(&mut self, _strategy: Option<lingq_upload_lib::core::epub::EpubVendor>) {
        self.events.lock().unwrap().push(Event::Started);
    }
    fn progress(&mut self, _pct: f32, _message: Option<String>) {
        self.events.lock().unwrap().push(Event::Progress);
    }
    fn chapter_done(&mut self, _chapter_index: usize, _lesson_id: i64, degraded: bool) {
        self.events
            .lock()
            .unwrap()
            .push(Event::ChapterDone { degraded });
    }
    fn cancelled(&mut self) {
        self.events.lock().unwrap().push(Event::Cancelled);
    }
    fn result(&mut self, ok: bool, _payload: serde_json::Value) {
        self.events.lock().unwrap().push(Event::Result(ok));
    }
    fn needs_match(
        &mut self,
        _title: String,
        chapters: usize,
        tracks: usize,
        condition: MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
        bucket_preview: Option<Vec<BucketPreview>>,
    ) {
        self.events
            .lock()
            .unwrap()
            .push(Event::NeedsMatch(NeedsMatchPayload {
                chapters,
                tracks,
                condition,
                options,
                preselect,
                bucket_preview,
            }));
    }
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

fn build_project(
    title: &str,
    audio: AudioSource,
    text_paths: Vec<PathBuf>,
) -> (ProjectId, Project) {
    let id = ProjectId::from_title_author(title, "Author");
    let project = Project {
        schema_version: SCHEMA_V1,
        id: id.clone(),
        sources: ProjectSources {
            text: TextSource::LooseFiles { paths: text_paths },
            audio: Some(audio),
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
        authors: vec![],
        series: None,
        lingq_collection_id: None,
        last_activity_at: None,
        stage: Default::default(),
        last_transition_at: None,
        skipped_chapters: vec![],
        absorb_policy: AbsorbPolicy::default(),
        mapping: None,
        confirmed_at: None,
        cover_use: true,
        cover_uploaded_to_lingq: false,
        cover_source_href: None,
    };
    (id, project)
}

#[tokio::test]
async fn chaptered_m4b_routes_to_many_to_few_with_preview() {
    // synth_chapters_generic.m4b carries 3 atoms (~20 s each, 60 s total).
    // classify(7, 3): delta=4 skips CountOff; 2*7=14 > 3*3=9 → ManyToFew.
    let m4b = fixture_path("synth_chapters_generic.m4b");
    assert!(m4b.exists(), "missing fixture {}", m4b.display());
    let text_dir = tempfile::tempdir().unwrap();
    let text_paths = make_chapter_files(text_dir.path(), 7);
    let text_dir_keep = text_dir; // hold the temp until run finishes

    let (project_id, project) =
        build_project("Chaptered M4B", AudioSource::SingleFile(m4b), text_paths);
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());
    store.put(&project).unwrap();

    let server = Server::new_async().await;
    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        LanguageCode::new("ja").expect("valid lang"),
        server.url(),
    ));
    let mut sink = RecordingSink::default();
    run_project_job(
        store.clone(),
        client,
        project_id.clone(),
        CancellationToken::new(),
        &mut sink,
    )
    .await
    .expect("orchestrator run");

    let events = sink.events.lock().unwrap().clone();
    let payload = events
        .iter()
        .find_map(|e| match e {
            Event::NeedsMatch(p) => Some(p.clone()),
            _ => None,
        })
        .expect("expected NeedsMatch event");

    assert_eq!(payload.chapters, 7);
    assert_eq!(payload.tracks, 3);
    assert_eq!(payload.condition, MismatchCondition::ManyToFew);
    assert_eq!(payload.preselect, MismatchResponse::SplitProportional);
    assert_eq!(
        payload.options,
        vec![
            MismatchResponse::SplitProportional,
            MismatchResponse::SingleLesson,
            MismatchResponse::Cancel,
        ],
    );

    let buckets = payload
        .bucket_preview
        .as_ref()
        .expect("ManyToFew must carry bucket_preview");
    assert_eq!(buckets.len(), 3, "preview should have one row per atom");

    // Contiguous coverage of 0..7 with strictly-monotonic boundaries.
    assert_eq!(buckets[0].text_range_start, 0);
    assert_eq!(buckets.last().unwrap().text_range_end, 7);
    let mut cursor = 0usize;
    for (i, b) in buckets.iter().enumerate() {
        assert_eq!(b.text_range_start, cursor, "bucket {i} start");
        assert!(
            b.text_range_end >= b.text_range_start,
            "bucket {i} reversed",
        );
        cursor = b.text_range_end;
        assert!(
            b.chars_per_sec.is_finite() && b.chars_per_sec >= 0.0,
            "bucket {i} chars_per_sec must be finite + non-negative, got {}",
            b.chars_per_sec,
        );
        assert!(
            b.atom_duration_sec > 0.0,
            "bucket {i} atom_duration_sec must be positive, got {}",
            b.atom_duration_sec,
        );
    }
    assert_eq!(cursor, 7);

    // NeedsMatch is terminal: no chapter uploads should have happened.
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::ChapterDone { .. })),
        "no uploads should fire before user resolves match",
    );

    drop(text_dir_keep);
}

#[tokio::test]
async fn libation_folder_does_not_invoke_atom_probe() {
    // Folder source = per-track delivery (population A in the spec): probe
    // never runs. Equal counts (3 vs 3) take the clean Paired path; the
    // orchestrator never emits NeedsMatch, so no bucket_preview is ever
    // produced.
    let audio_dir = tempfile::tempdir().unwrap();
    let probe = fixture_path("probe_3min.mp3");
    for i in 0..3 {
        let dst = audio_dir.path().join(format!("track_{:02}.mp3", i + 1));
        std::fs::copy(&probe, &dst).unwrap();
    }
    let text_dir = tempfile::tempdir().unwrap();
    let text_paths = make_chapter_files(text_dir.path(), 3);

    let (project_id, project) = build_project(
        "Libation Folder",
        AudioSource::Folder(audio_dir.path().to_path_buf()),
        text_paths,
    );
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());
    store.put(&project).unwrap();

    let mut server = Server::new_async().await;
    let _coll = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/api/v3/ja/collections/\?search=".into()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results":[{"pk":9001,"title":"Libation Folder"}]}"#)
        .create();
    for i in 0..3 {
        let _ = server
            .mock("POST", "/api/v3/ja/lessons/import/")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(format!(r#"{{"pk":{}}}"#, 7000 + i))
            .expect(1)
            .create();
    }

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        LanguageCode::new("ja").expect("valid lang"),
        server.url(),
    ));
    let mut sink = RecordingSink::default();
    run_project_job(
        store.clone(),
        client,
        project_id.clone(),
        CancellationToken::new(),
        &mut sink,
    )
    .await
    .expect("orchestrator run");

    let events = sink.events.lock().unwrap().clone();
    assert!(
        !events.iter().any(|e| matches!(e, Event::NeedsMatch(_))),
        "clean pair must not emit NeedsMatch (and therefore no bucket_preview)",
    );
    let done: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::ChapterDone { degraded } => Some(*degraded),
            _ => None,
        })
        .collect();
    assert_eq!(done.len(), 3, "expected 3 uploads, got events {:?}", events);
    assert!(done.iter().all(|d| !d), "clean pair must not be degraded");
    assert!(matches!(events.last(), Some(Event::Result(true))));

    let project = store.get(&project_id).unwrap().unwrap();
    assert_eq!(project.receipts.len(), 3);

    let _ = audio_dir;
}

#[tokio::test]
async fn atomless_single_m4b_falls_back_to_whole_file() {
    // Population D: a single-file audio source with no embedded chapters.
    // `expand_single_file` must return one whole-file track (window == None)
    // and the matcher must see (1 chapter, 1 track) → clean pair.
    let audio_dir = tempfile::tempdir().unwrap();
    let silence = audio_dir.path().join("silence.wav");
    support::mk_fixture::write_silence_m4a_like(&silence, 5);
    let text_dir = tempfile::tempdir().unwrap();
    let text_paths = make_chapter_files(text_dir.path(), 1);

    let (project_id, project) =
        build_project("Atomless", AudioSource::SingleFile(silence), text_paths);
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());
    store.put(&project).unwrap();

    let mut server = Server::new_async().await;
    let _coll = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/api/v3/ja/collections/\?search=".into()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results":[{"pk":424242,"title":"Atomless"}]}"#)
        .create();
    let _import = server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"pk":91001}"#)
        .expect(1)
        .create();

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        LanguageCode::new("ja").expect("valid lang"),
        server.url(),
    ));
    let mut sink = RecordingSink::default();
    run_project_job(
        store.clone(),
        client,
        project_id.clone(),
        CancellationToken::new(),
        &mut sink,
    )
    .await
    .expect("orchestrator run");

    let events = sink.events.lock().unwrap().clone();
    assert!(
        !events.iter().any(|e| matches!(e, Event::NeedsMatch(_))),
        "atomless single-file must take the clean (1, 1) path",
    );
    let done = events
        .iter()
        .filter(|e| matches!(e, Event::ChapterDone { .. }))
        .count();
    assert_eq!(done, 1, "expected one chapter upload, got {:?}", events);
    assert!(matches!(events.last(), Some(Event::Result(true))));

    let project = store.get(&project_id).unwrap().unwrap();
    assert_eq!(project.receipts.len(), 1);
    assert!(!project.receipts[0].degraded);
}

#[tokio::test]
async fn windowed_transcode_catches_duration_mismatch() {
    let src = fixture_path("synth_chapters_generic.m4b");
    assert!(src.exists(), "missing fixture {}", src.display());

    let tmp = tempfile::tempdir().unwrap();
    let enc = audio::EncoderSettings::default();

    // Honest window: 0..10 of a 60 s source. The transcoder slices to ~10 s;
    // the duration verify must accept it within 1.0 s.
    let dst_ok = tmp.path().join("slice_ok.mp3");
    let report = audio::transcode(&src, &dst_ok, &enc, Some((0.0, 10.0)))
        .await
        .expect("honest windowed transcode");
    assert!(
        (report.dst_duration_sec - 10.0).abs() < 1.0,
        "honest window: dst duration {} should be within 1 s of 10",
        report.dst_duration_sec,
    );

    // Bogus window: claims 999 s of audio against a 60 s source. The actual
    // output is bounded by the source (~60 s) so the verify step must reject
    // it as DurationMismatch.
    let dst_bad = tmp.path().join("slice_bad.mp3");
    let err = audio::transcode(&src, &dst_bad, &enc, Some((0.0, 999.0)))
        .await
        .expect_err("bogus window must fail duration verify");
    assert!(
        matches!(err, AudioError::DurationMismatch { .. }),
        "expected DurationMismatch, got {:?}",
        err,
    );
}
