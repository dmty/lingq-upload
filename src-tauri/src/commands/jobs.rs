//! Tauri commands to start and cancel project-wide upload jobs.
//!
//! The orchestrator itself lives in [`crate::core::job`] and is
//! tauri-agnostic so it can be unit-tested without a running app handle.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use secrecy::SecretString;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::core::identity::ProjectId;
use crate::core::job::{run_project_job, JobSink};
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::events::{JobEmitter, Stage};
use crate::lingq::LingqClient;
use crate::secrets::{RealKeyring, SecretsStore};

/// Singleton map of active job cancellation tokens. Managed in `lib.rs::run`.
pub type JobCancelMap = Arc<Mutex<HashMap<Uuid, CancellationToken>>>;

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
    let lang = project.settings.language.clone();
    let client = Arc::new(LingqClient::new(SecretString::from(key), lang));

    let job_id = Uuid::new_v4();
    let token = CancellationToken::new();
    cancels
        .lock()
        .expect("job cancel map mutex poisoned")
        .insert(job_id, token.clone());

    let store_ref: Arc<dyn ProjectStore> = store.inner().clone();
    let cancels_ref: JobCancelMap = cancels.inner().clone();
    let app_for_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let mut emitter = JobEmitter::new(&app_for_task, job_id);
        let mut sink = EmitterSink { inner: &mut emitter };
        if let Err(e) =
            run_project_job(store_ref, client, project_id, token, &mut sink).await
        {
            tracing::error!(job_id = %job_id, error = %e, "project job failed");
        }
        cancels_ref
            .lock()
            .expect("job cancel map mutex poisoned")
            .remove(&job_id);
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
    let maybe = cancels
        .lock()
        .expect("job cancel map mutex poisoned")
        .get(&job_id)
        .cloned();
    if let Some(tok) = maybe {
        tok.cancel();
        tracing::info!(job_id = %job_id, "cancel signalled");
    } else {
        tracing::info!(job_id = %job_id, "cancel ignored: no active job");
    }
    Ok(())
}

/// Bridge between the orchestrator's [`JobSink`] trait and the runtime
/// [`JobEmitter`]. Lives here so `core::job` can stay tauri-free.
struct EmitterSink<'a, 'b> {
    inner: &'a mut JobEmitter<'b>,
}

impl<'a, 'b> JobSink for EmitterSink<'a, 'b> {
    fn started(&mut self) {
        self.inner.started(Stage::Uploading);
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
}
