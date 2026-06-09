//! Tauri commands to start and cancel project-wide upload jobs.
//!
//! The orchestrator itself lives in [`crate::core::job`] and is
//! tauri-agnostic so it can be unit-tested without a running app handle.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use secrecy::SecretString;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use chrono::{DateTime, Utc};

use super::parse_lang;
use crate::core::identity::ProjectId;
use crate::core::job::{run_project_job, JobSink};
use crate::core::matcher::{BucketPreview, MismatchCondition, MismatchResponse};
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::events::{JobEmitter, Stage};
use crate::lingq::LingqClient;
use crate::secrets::{RealKeyring, SecretsStore};

/// Lightweight projection of a [`crate::core::project::ChapterReceipt`] for
/// rehydration. Includes only the fields the Run screen needs to render chips.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, PartialEq)]
pub struct ReceiptSnapshot {
    pub chapter_index: usize,
    pub lesson_id: Option<i64>,
    pub degraded: bool,
    pub uploaded_at: Option<DateTime<Utc>>,
}

/// Tauri-free core of [`cmd_replay_receipts`]. Returns receipts in chapter
/// order; gaps in `chapter_index` are not filled — the caller sees exactly
/// what's persisted.
pub fn replay_receipts_impl(
    store: &dyn ProjectStore,
    project_id: &ProjectId,
) -> Result<Vec<ReceiptSnapshot>, AppError> {
    let project = store
        .get(project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other(format!("project not found: {}", project_id.join_key())))?;
    let mut snaps: Vec<ReceiptSnapshot> = project
        .receipts
        .iter()
        .map(|r| ReceiptSnapshot {
            chapter_index: r.chapter_index,
            lesson_id: r.lesson_id,
            degraded: r.degraded,
            uploaded_at: r.uploaded_at,
        })
        .collect();
    snaps.sort_by_key(|s| s.chapter_index);
    Ok(snaps)
}

/// Replay persisted receipts so the Run screen can render already-uploaded
/// chapters as green chips immediately on cold rehydration, without re-running
/// the job.
#[tauri::command]
#[specta::specta]
pub async fn cmd_replay_receipts(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
) -> Result<Vec<ReceiptSnapshot>, AppError> {
    replay_receipts_impl(store.inner().as_ref(), &project_id)
}

/// Each entry pins both the cancellation token AND the project id so we can
/// answer "is this project already running?" without scanning the store.
type JobCancelEntry = (ProjectId, CancellationToken);

/// Singleton map of active job cancellation tokens. Managed in `lib.rs::run`.
pub type JobCancelMap = Arc<Mutex<HashMap<Uuid, JobCancelEntry>>>;

/// Single funnel for locking the cancel map. The map is single-process and
/// a poisoned mutex means real corruption — propagate the panic.
pub(crate) fn lock_cancels(map: &JobCancelMap) -> MutexGuard<'_, HashMap<Uuid, JobCancelEntry>> {
    map.lock().expect("job cancel map mutex poisoned")
}

fn is_project_active(map: &HashMap<Uuid, JobCancelEntry>, project_id: &ProjectId) -> bool {
    map.values().any(|(pid, _)| pid == project_id)
}

/// RAII guard that removes a job from the cancel map on drop. A panic inside
/// the spawned task would otherwise leak the entry forever — the explicit
/// `.remove()` only runs on the happy / Err return paths.
struct JobMapGuard {
    map: JobCancelMap,
    job_id: Uuid,
}

impl JobMapGuard {
    fn new(map: JobCancelMap, job_id: Uuid) -> Self {
        Self { map, job_id }
    }
}

impl Drop for JobMapGuard {
    fn drop(&mut self) {
        lock_cancels(&self.map).remove(&self.job_id);
    }
}

/// Start an end-to-end project job. Returns immediately with the new job id;
/// the actual work runs on the tokio runtime and streams `JobEvent`s.
#[tauri::command]
#[specta::specta]
pub async fn cmd_start_project_job(
    app: AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    cancels: tauri::State<'_, JobCancelMap>,
    project_id: ProjectId,
) -> Result<Uuid, AppError> {
    let secrets = SecretsStore::new(Box::new(RealKeyring::new()));
    let key = secrets.load_key()?.ok_or(AppError::MissingApiKey)?;

    let project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    let lang = parse_lang(&project.settings.language)?;
    let client = Arc::new(LingqClient::new(SecretString::from(key), lang));

    let job_id = Uuid::new_v4();
    let token = CancellationToken::new();
    {
        // Reject duplicate-start. Two rapid clicks would otherwise race on
        // the same project.json and trash receipts.
        let mut guard = lock_cancels(cancels.inner());
        if is_project_active(&guard, &project_id) {
            return Err(AppError::Other("project already running".into()));
        }
        guard.insert(job_id, (project_id.clone(), token.clone()));
    }

    let store_ref: Arc<dyn ProjectStore> = store.inner().clone();
    let cancels_ref: JobCancelMap = cancels.inner().clone();
    let app_for_task = app.clone();

    tauri::async_runtime::spawn(async move {
        // Guarantee map cleanup even if the future panics.
        let _guard = JobMapGuard::new(cancels_ref, job_id);
        let mut emitter = JobEmitter::new(&app_for_task, job_id);
        let mut sink = EmitterSink {
            inner: &mut emitter,
        };
        if let Err(e) = run_project_job(store_ref, client, project_id, token, &mut sink).await {
            tracing::error!(job_id = %job_id, error = %e, "project job failed");
        }
    });

    Ok(job_id)
}

/// Signal cancellation for a previously started job. No-op if the job has
/// already finished and the token was reaped.
#[tauri::command]
#[specta::specta]
pub async fn cmd_cancel_job(
    cancels: tauri::State<'_, JobCancelMap>,
    job_id: Uuid,
) -> Result<(), AppError> {
    let maybe = lock_cancels(cancels.inner())
        .get(&job_id)
        .map(|(_, tok)| tok.clone());
    if let Some(tok) = maybe {
        tok.cancel();
        tracing::info!(job_id = %job_id, "cancel signalled");
    } else {
        tracing::info!(job_id = %job_id, "cancel ignored: no active job");
    }
    Ok(())
}

/// Tauri-free core of [`cmd_project_cancel`]. Iterates the map and fires every
/// token whose entry matches `project_id`. Returns the number of tokens fired.
///
/// The return value is for tests and `tracing` diagnostics — the frontend
/// ignores it; cancellation is fire-and-forget from the UI's view.
pub fn cancel_project_impl(map: &JobCancelMap, project_id: &ProjectId) -> usize {
    let tokens: Vec<CancellationToken> = {
        let guard = lock_cancels(map);
        guard
            .values()
            .filter(|(pid, _)| pid == project_id)
            .map(|(_, tok)| tok.clone())
            .collect()
    };
    let fired = tokens.len();
    for tok in tokens {
        tok.cancel();
    }
    fired
}

/// Signal cancellation for every active job whose project id matches. Lets the
/// Run screen cancel without first knowing the server-issued `job_id` — the
/// jobless start-race window and post-reload state both lose that handle.
#[tauri::command]
#[specta::specta]
pub async fn cmd_project_cancel(
    cancels: tauri::State<'_, JobCancelMap>,
    project_id: ProjectId,
) -> Result<usize, AppError> {
    let fired = cancel_project_impl(cancels.inner(), &project_id);
    tracing::info!(project = %project_id.join_key(), fired, "project cancel signalled");
    Ok(fired)
}

/// Bridge between the orchestrator's [`JobSink`] trait and the runtime
/// [`JobEmitter`]. Lives here so `core::job` can stay tauri-free.
struct EmitterSink<'a, 'b> {
    inner: &'a mut JobEmitter<'b>,
}

impl<'a, 'b> JobSink for EmitterSink<'a, 'b> {
    fn started(&mut self, strategy: Option<&str>) {
        self.inner
            .started(Stage::Uploading, strategy.map(|s| s.to_string()));
    }
    fn progress(&mut self, pct: f32, message: Option<String>) {
        self.inner.progress(pct, message);
    }
    fn chapter_done(&mut self, chapter_index: usize, lesson_id: i64, degraded: bool) {
        self.inner.chapter_done(chapter_index, lesson_id, degraded);
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
