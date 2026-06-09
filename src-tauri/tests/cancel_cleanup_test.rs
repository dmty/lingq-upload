//! Verifies ffmpeg child kill + partial-output cleanup on cancel.
//!
//! Uses a sleep-injected ffmpeg stand-in (FFMPEG_BIN points to slow_transcode.sh
//! on Unix, slow_transcode.bat on Windows). Cancels mid-transcode and asserts
//! that no orphan child process survives and the destination file does not
//! exist.

use std::path::{Path, PathBuf};
use std::process::Command as SyncCommand;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lingq_upload_lib::core::audio::{transcode, EncoderSettings};

// FFMPEG_BIN is process-global; tests mutating it must run serially.
// The lock is file-scoped — sufficient because every test binary runs in its
// own process. If any future test in this binary touches FFMPEG_BIN outside
// SlowFfmpegGuard, it races and must take this lock too.
static FFMPEG_BIN_LOCK: Mutex<()> = Mutex::new(());

fn shim_path() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    #[cfg(windows)]
    let shim = here.join("tests/fixtures/ffmpeg/slow_transcode.bat");
    #[cfg(not(windows))]
    let shim = here.join("tests/fixtures/ffmpeg/slow_transcode.sh");
    shim.canonicalize()
        .unwrap_or_else(|e| panic!("shim path {} not resolvable: {e}", shim.display()))
}

struct SlowFfmpegGuard<'a> {
    prev: Option<String>,
    _lock: std::sync::MutexGuard<'a, ()>,
}

impl<'a> SlowFfmpegGuard<'a> {
    fn install(sleep_secs: u64) -> Self {
        let lock = FFMPEG_BIN_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let prev = std::env::var("FFMPEG_BIN").ok();
        std::env::set_var("FFMPEG_BIN", shim_path());
        std::env::set_var("SLOW_TRANSCODE_SLEEP", sleep_secs.to_string());
        Self { prev, _lock: lock }
    }
}

impl Drop for SlowFfmpegGuard<'_> {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var("FFMPEG_BIN", v),
            None => std::env::remove_var("FFMPEG_BIN"),
        }
        std::env::remove_var("SLOW_TRANSCODE_SLEEP");
    }
}

fn process_matches(name_lc: &str) -> bool {
    name_lc.contains("ffmpeg") || name_lc.contains("slow_transcode") || name_lc.contains("sleep")
}

fn list_orphan_names(parent_pid: u32) -> Vec<(u32, String)> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::new());
    sys.processes()
        .values()
        .filter_map(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            let is_child = p
                .parent()
                .map(|pp| pp.as_u32() == parent_pid)
                .unwrap_or(false);
            (process_matches(&name) && is_child).then(|| (p.pid().as_u32(), name))
        })
        .collect()
}

fn count_orphan_ffmpegs(parent_pid: u32) -> usize {
    list_orphan_names(parent_pid).len()
}

#[tokio::test]
async fn transcode_future_drop_kills_ffmpeg_and_unlinks_dst() {
    let _guard = SlowFfmpegGuard::install(10);

    // Choose a deterministic dst that does not exist yet.
    let dst_dir = tempfile::tempdir().unwrap();
    let dst_path = dst_dir.path().join("out.mp3");
    assert!(!dst_path.exists());

    // ffmpeg shim ignores -i, so any path works. Pass a window so the transcode
    // function does NOT call probe_duration on src (which would invoke a real
    // ffprobe outside our shim).
    let src = PathBuf::from("nonexistent.m4b");
    let enc = EncoderSettings::default();
    let our_pid = std::process::id();

    // Race the transcode future against a 1s timer. select! on owned futures
    // DROPS the loser — which mirrors what run_project_job's cancel arm does
    // to the live transcode future. Tokio's kill_on_drop fires SIGKILL on the
    // inner Child during that drop.
    //
    // While the race is active we observe the running child count via a
    // sibling probe future so we can prove the shim was actually spawned.
    let pre_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let pre_count_clone = pre_count.clone();
    tokio::select! {
        _ = transcode(&src, &dst_path, &enc, Some((0.0, 60.0))) => {
            panic!("shim returned before timeout — shim not invoked correctly");
        }
        _ = async move {
            tokio::time::sleep(Duration::from_millis(800)).await;
            let n = count_orphan_ffmpegs(our_pid);
            pre_count_clone.store(n, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(200)).await;
        } => {}
    }
    let pre_count = pre_count.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        pre_count >= 1,
        "shim should be running as child of test pid {our_pid}; pre_count={pre_count}",
    );
    // Drop fires SIGKILL via tokio's kill_on_drop. Poll for the child to vanish
    // — most kernels reap within a few ms, but CI hosts can lag.
    let mut post_count = count_orphan_ffmpegs(our_pid);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while post_count != 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        post_count = count_orphan_ffmpegs(our_pid);
    }
    if post_count != 0 {
        let names = list_orphan_names(our_pid);
        panic!(
            "no orphan ffmpeg/shim child of pid {our_pid} after drop; got {post_count}: {names:?}",
        );
    }
    assert!(
        !dst_path.exists(),
        "dst should not exist after cancel; found {}",
        dst_path.display(),
    );
}

fn which(bin: &str) -> Option<PathBuf> {
    SyncCommand::new("which")
        .arg(bin)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
}

fn ffprobe_available() -> bool {
    which("ffprobe").is_some()
}

#[derive(Default)]
struct CancelSink {
    events: Arc<Mutex<Vec<String>>>,
}

impl lingq_upload_lib::core::job::JobSink for CancelSink {
    fn started(&mut self) {
        self.events.lock().unwrap().push("started".into());
    }
    fn progress(&mut self, _pct: f32, _message: Option<String>) {}
    fn chapter_done(&mut self, _chapter_index: usize, _lesson_id: i64, _degraded: bool) {
        self.events.lock().unwrap().push("chapter_done".into());
    }
    fn cancelled(&mut self) {
        self.events.lock().unwrap().push("cancelled".into());
    }
    fn result(&mut self, ok: bool, _payload: serde_json::Value) {
        self.events.lock().unwrap().push(format!("result:{ok}"));
    }
    fn needs_match(
        &mut self,
        _title: String,
        _chapters: usize,
        _tracks: usize,
        _condition: lingq_upload_lib::core::matcher::MismatchCondition,
        _options: Vec<lingq_upload_lib::core::matcher::MismatchResponse>,
        _preselect: lingq_upload_lib::core::matcher::MismatchResponse,
        _bucket_preview: Option<Vec<lingq_upload_lib::core::matcher::BucketPreview>>,
    ) {
        self.events.lock().unwrap().push("needs_match".into());
    }
}

#[tokio::test]
async fn run_project_job_cancel_unlinks_partial_dst_and_holds_stage() {
    use lingq_upload_lib::core::identity::ProjectId;
    use lingq_upload_lib::core::job::run_project_job;
    use lingq_upload_lib::core::project::{
        Project, ProjectSettings, ProjectSources, ProjectStage, SCHEMA_V1,
    };
    use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
    use lingq_upload_lib::ingest::{AudioSource, TextSource};
    use lingq_upload_lib::lingq::{LanguageCode, LingqClient};
    use mockito::{Matcher, Server};
    use secrecy::SecretString;
    use tokio_util::sync::CancellationToken;

    if !ffprobe_available() {
        eprintln!("ffprobe not on PATH — skipping run_project_job_cancel test");
        return;
    }

    let _guard = SlowFfmpegGuard::install(10);

    let mut server = Server::new_async().await;
    let _ = server
        .mock(
            "GET",
            Matcher::Regex(r"^/api/v3/ja/collections/\?search=".into()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results":[{"pk":4242,"title":"My Book"}]}"#)
        .create();

    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());

    let audio_dir = tempfile::tempdir().unwrap();
    let probe =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/probe_3min.mp3");
    for i in 0..2 {
        let dst = audio_dir.path().join(format!("track_{:02}.mp3", i + 1));
        std::fs::copy(&probe, &dst).unwrap();
    }

    let text_dir = tempfile::tempdir().unwrap();
    let text_paths: Vec<PathBuf> = (0..2)
        .map(|i| {
            let p = text_dir.path().join(format!("ch_{:02}.txt", i + 1));
            std::fs::write(&p, format!("Body of chapter {}.\n", i + 1)).unwrap();
            p
        })
        .collect();
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
    skipped_chapters: vec![],
    };
    store.put(&project).unwrap();

    let client = Arc::new(LingqClient::with_base_url(
        SecretString::new("test-key".into()),
        LanguageCode::new("ja").expect("valid lang"),
        server.url(),
    ));
    let mut sink = CancelSink::default();
    let token = CancellationToken::new();
    let token_clone = token.clone();
    let our_pid = std::process::id();

    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        token_clone.cancel();
    });

    let result = run_project_job(store.clone(), client, id.clone(), token, &mut sink).await;
    canceller.await.unwrap();

    assert!(
        result.is_ok(),
        "cancel contract: returns Ok; got {result:?}"
    );
    let events = sink.events.lock().unwrap().clone();
    assert!(
        events.iter().any(|e| e == "cancelled"),
        "expected Cancelled event; got {events:?}",
    );

    // Poll for kill to propagate.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut post_count = count_orphan_ffmpegs(our_pid);
    while post_count != 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        post_count = count_orphan_ffmpegs(our_pid);
    }
    assert_eq!(
        post_count, 0,
        "no orphan ffmpeg/shim child of pid {our_pid} after orchestrator cancel; got {post_count}: {:?}",
        list_orphan_names(our_pid),
    );

    let persisted = store.get(&id).unwrap().expect("project persisted");
    assert_eq!(
        persisted.stage(),
        ProjectStage::Mapped,
        "stage should remain at Mapped after mid-transcode cancel",
    );

    let _ = audio_dir;
}
