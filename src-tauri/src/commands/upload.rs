use std::path::PathBuf;

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use uuid::Uuid;

use super::{app_data_dir, parse_lang};
use crate::core::{audio, text};
use crate::error::AppError;
use crate::events::{JobEmitter, Stage};
use crate::ingest::{audio_source_paths, Candidate, TextSource};
use crate::lingq::{LessonOpts, LingqClient};
use crate::secrets::SecretsStore;

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

    let TextSource::Epub(text_path) = candidate.text_source.clone() else {
        return Err(AppError::Unsupported(
            "one-shot upload requires an EPUB-derived text source".into(),
        ));
    };

    let Some(audio_source) = candidate.audio_source.as_ref() else {
        return Err(AppError::Unsupported(
            "one-shot upload requires an audio source".into(),
        ));
    };
    let audio_paths = audio_source_paths(audio_source)?;
    let [audio_path] = <[PathBuf; 1]>::try_from(audio_paths).map_err(|paths| {
        AppError::Unsupported(format!(
            "one-shot upload requires a single audio file (got {})",
            paths.len()
        ))
    })?;

    let store = SecretsStore::new_default(&app_data_dir(&app)?);
    let key = store.load_key()?.ok_or(AppError::MissingApiKey)?;

    let strategy = match crate::core::epub::autodetect_vendor(&text_path) {
        Ok(d) => Some(d.vendor),
        Err(_) => Some(crate::core::epub::EpubVendor::Generic),
    };
    job.started(Stage::Parsing, strategy);
    job.progress(0.0, Some("Reading text".into()));
    let text_body = text::read_text_for_upload(&text_path)?;

    // tempdir lives until the upload finishes so the staged mp3 isn't
    // unlinked mid-upload. Source-adjacent writes break on read-only mounts.
    let staging = tempfile::tempdir()?;
    let audio_for_upload: PathBuf =
        if audio_path.extension().and_then(|e| e.to_str()) == Some("mp3") {
            audio_path
        } else {
            job.stage(Stage::Transcoding);
            job.progress(0.0, Some("Transcoding audio".into()));
            let dst = staging.path().join("upload.mp3");
            let _report = audio::transcode(&audio_path, &dst, &Default::default(), None).await?;
            job.progress(1.0, Some("Transcode complete".into()));
            dst
        };

    job.stage(Stage::Uploading);
    job.progress(0.0, Some("Uploading to LingQ".into()));

    let code = parse_lang(&lang)?;
    let client = LingqClient::new(SecretString::from(key), code);
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
