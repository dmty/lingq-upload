use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::core::{audio, text};
use crate::error::AppError;
use crate::events::{JobEvent, Stage};
use crate::ingest::{AudioSource, Candidate, TextSource};
use crate::lingq::{LessonOpts, LingqClient};
use crate::secrets::{RealKeyring, SecretsStore};

#[derive(Debug, Serialize, Deserialize, Type, Clone)]
pub struct UploadResult {
    pub lesson_id: i64,
    pub lesson_url: String,
}

fn emit(app: &AppHandle, event: JobEvent) {
    if let Err(e) = app.emit("job", event) {
        tracing::warn!(error = %e, "JobEvent emit dropped");
    }
}

#[tauri::command]
#[specta::specta]
pub async fn upload_one_shot(
    app: AppHandle,
    candidate: Candidate,
    collection_id: i64,
    lang: String,
) -> Result<UploadResult, AppError> {
    let job_id = Uuid::new_v4();

    let text_path = match &candidate.text_source {
        TextSource::Epub(p) => p.clone(),
        TextSource::LooseFiles { .. } => {
            return Err(AppError::Internal(
                "LooseFiles text source not supported in one-shot upload".into(),
            ));
        }
    };

    let audio_path = match candidate.audio_source.as_ref() {
        Some(AudioSource::SingleFile(p)) => p.clone(),
        Some(_) => {
            return Err(AppError::Internal(
                "non-single-file audio source not supported in one-shot upload".into(),
            ));
        }
        None => {
            return Err(AppError::Internal(
                "candidate has no audio_source".into(),
            ));
        }
    };

    let store = SecretsStore::new(Box::new(RealKeyring::new()));
    let key = store
        .load_key()?
        .ok_or_else(|| AppError::Internal("no LingQ API key set; configure it in Settings".into()))?;

    emit(
        &app,
        JobEvent::Started {
            job_id,
            stage: Stage::Parsing,
        },
    );
    emit(
        &app,
        JobEvent::Progress {
            job_id,
            pct: 0.0,
            message: Some("Reading text".into()),
        },
    );
    let text_body = text::read_text_for_upload(&text_path)?;

    // S1.4 thin slice: write transcoded mp3 beside the source.
    // Production should use a temp dir; the IngestSource boundary will own
    // staging in E2.
    let audio_for_upload = if audio_path.extension().and_then(|e| e.to_str())
        == Some("mp3")
    {
        audio_path
    } else {
        emit(
            &app,
            JobEvent::Started {
                job_id,
                stage: Stage::Transcoding,
            },
        );
        emit(
            &app,
            JobEvent::Progress {
                job_id,
                pct: 0.0,
                message: Some("Transcoding audio".into()),
            },
        );
        let dst = audio_path.with_extension("mp3");
        let _report = audio::transcode(&audio_path, &dst, &Default::default()).await?;
        emit(
            &app,
            JobEvent::Progress {
                job_id,
                pct: 1.0,
                message: Some("Transcode complete".into()),
            },
        );
        dst
    };

    emit(
        &app,
        JobEvent::Started {
            job_id,
            stage: Stage::Uploading,
        },
    );
    emit(
        &app,
        JobEvent::Progress {
            job_id,
            pct: 0.0,
            message: Some("Uploading to LingQ".into()),
        },
    );

    let client = LingqClient::new(SecretString::from(key), lang.as_str());
    let lesson_id = client
        .import_lesson(
            collection_id,
            &candidate.title,
            &text_body,
            &audio_for_upload,
            &LessonOpts::default(),
        )
        .await?;

    let result = UploadResult {
        lesson_id,
        lesson_url: format!(
            "https://www.lingq.com/{lang}/learn/{lang}/web/library/course/{collection_id}",
        ),
    };

    emit(
        &app,
        JobEvent::Result {
            job_id,
            ok: true,
            payload: serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
        },
    );

    Ok(result)
}
