use std::time::Duration;

use tauri::AppHandle;
use uuid::Uuid;

use crate::error::AppError;
use crate::events::{JobEmitter, Stage};

#[tauri::command]
#[specta::specta]
pub async fn start_demo_job(app: AppHandle) -> Result<Uuid, AppError> {
    let job_id = Uuid::new_v4();
    tauri::async_runtime::spawn(async move {
        let mut emitter = JobEmitter::new(&app, job_id);
        emitter.started(Stage::Transcoding, None);
        for i in 1..=5u32 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            emitter.progress(i as f32 / 5.0, None);
        }
        emitter.result(true, serde_json::json!({ "demo": true }));
    });
    Ok(job_id)
}
