use std::path::PathBuf;

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use uuid::Uuid;

use crate::core::{audio, text};
use crate::error::AppError;
use crate::events::{JobEmitter, Stage};
use crate::ingest::{AudioSource, Candidate, TextSource};
use crate::lingq::{LessonOpts, LingqClient};
use crate::secrets::{RealKeyring, SecretsStore};

#[derive(Debug, Serialize, Deserialize, Type, Clone)]
pub struct UploadResult {
    pub lesson_id: i64,
    pub lesson_url: String,
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
    let mut job = JobEmitter::new(&app, job_id);

    let text_path = match &candidate.text_source {
        TextSource::Epub(p) => p.clone(),
        TextSource::LooseFiles { .. } => {
            return Err(AppError::Unsupported(
                "LooseFiles text source not supported in one-shot upload".into(),
            ));
        }
    };

    let audio_path = match candidate.audio_source.as_ref() {
        Some(AudioSource::SingleFile(p)) => p.clone(),
        Some(AudioSource::Folder(_)) => {
            return Err(AppError::Unsupported(
                "folder audio source not supported in one-shot upload".into(),
            ));
        }
        Some(AudioSource::LibationManifest(_)) => {
            return Err(AppError::Unsupported(
                "libation manifest audio source not supported in one-shot upload".into(),
            ));
        }
        None => {
            return Err(AppError::Unsupported(
                "candidate has no audio_source".into(),
            ));
        }
    };

    let store = SecretsStore::new(Box::new(RealKeyring::new()));
    let key = store.load_key()?.ok_or(AppError::MissingApiKey)?;

    job.started(Stage::Parsing);
    job.progress(0.0, Some("Reading text".into()));
    let text_body = text::read_text_for_upload(&text_path)?;

    // tempdir lives until the upload finishes so the staged mp3 isn't
    // unlinked mid-upload. Source-adjacent writes break on read-only mounts.
    let staging = tempfile::tempdir()?;
    let audio_for_upload: PathBuf = if audio_path.extension().and_then(|e| e.to_str())
        == Some("mp3")
    {
        audio_path
    } else {
        job.stage(Stage::Transcoding);
        job.progress(0.0, Some("Transcoding audio".into()));
        let dst = staging.path().join("upload.mp3");
        let _report = audio::transcode(&audio_path, &dst, &Default::default()).await?;
        job.progress(1.0, Some("Transcode complete".into()));
        dst
    };

    job.stage(Stage::Uploading);
    job.progress(0.0, Some("Uploading to LingQ".into()));

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

    let payload = serde_json::to_value(&result).expect("UploadResult serializes");
    job.result(true, payload);

    Ok(result)
}
