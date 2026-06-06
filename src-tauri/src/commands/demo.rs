use std::time::Duration;

use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::error::AppError;
use crate::events::{JobEvent, Stage};

fn emit(app: &AppHandle, event: JobEvent) {
    if let Err(e) = app.emit("job", event) {
        tracing::warn!(error = %e, "JobEvent emit dropped");
    }
}

#[tauri::command]
#[specta::specta]
pub async fn start_demo_job(app: AppHandle) -> Result<Uuid, AppError> {
    let job_id = Uuid::new_v4();
    tauri::async_runtime::spawn(async move {
        emit(
            &app,
            JobEvent::Started {
                job_id,
                stage: Stage::Transcoding,
            },
        );
        for i in 1..=5u32 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            emit(
                &app,
                JobEvent::Progress {
                    job_id,
                    pct: i as f32 / 5.0,
                    message: None,
                },
            );
        }
        emit(
            &app,
            JobEvent::Result {
                job_id,
                ok: true,
                payload: serde_json::json!({ "demo": true }),
            },
        );
    });
    Ok(job_id)
}
