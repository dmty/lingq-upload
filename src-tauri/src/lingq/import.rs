use std::path::Path;
use std::time::Duration;

use reqwest::multipart::{Form, Part};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::client::LingqClient;
use super::collections::CollectionId;
use super::error::LingqError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LessonStatus {
    Private,
    Public,
}

impl LessonStatus {
    fn as_form_str(self) -> &'static str {
        match self {
            LessonStatus::Private => "0",
            LessonStatus::Public => "1",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportLessonRequest<'a> {
    pub collection: CollectionId,
    pub title: &'a str,
    pub text: &'a str,
    pub audio: Option<&'a Path>,
    pub language: &'a str,
    pub level: u8,
    pub status: LessonStatus,
    pub tags: &'a [&'a str],
    pub save: bool,
}

impl LingqClient {
    /// Import a single lesson with full multipart fields. Per-call language
    /// override (AD-017). Retries on 5xx with capped exponential backoff
    /// (max 3 attempts); 4xx fails fast.
    pub async fn import_lesson_v2(
        &self,
        req: ImportLessonRequest<'_>,
    ) -> Result<i64, LingqError> {
        let url = format!("{}/api/v3/{}/lessons/import/", self.base_url(), req.language);
        let mut last_err: Option<LingqError> = None;
        for attempt in 0..3u32 {
            if attempt > 0 {
                let backoff = Duration::from_millis(200u64 << attempt);
                tokio::time::sleep(backoff).await;
            }
            let form = build_form(&req).await?;
            let resp = self
                .http()
                .post(&url)
                .header("Authorization", self.auth_header())
                .multipart(form)
                .send()
                .await
                .map_err(|e| LingqError::Transport(e.to_string()))?;
            let status = resp.status();
            match status {
                s if s.is_success() => return parse_lesson_id(resp).await,
                StatusCode::UNAUTHORIZED => return Err(LingqError::Unauthorized),
                StatusCode::NOT_FOUND => return Err(LingqError::NotFound),
                s if s.is_client_error() => {
                    let detail = resp.text().await.unwrap_or_default();
                    return Err(LingqError::BadRequest(detail));
                }
                s if s.is_server_error() => {
                    let detail = resp.text().await.unwrap_or_default();
                    last_err = Some(LingqError::Server(detail));
                }
                other => {
                    last_err = Some(LingqError::Transport(format!(
                        "unexpected status {other}"
                    )))
                }
            }
        }
        Err(last_err.unwrap_or_else(|| LingqError::Transport("retries exhausted".into())))
    }
}

async fn build_form(req: &ImportLessonRequest<'_>) -> Result<Form, LingqError> {
    let mut form = Form::new()
        .text("title", req.title.to_string())
        .text("text", req.text.to_string())
        .text("collection", req.collection.0.to_string())
        .text("language", req.language.to_string())
        .text("level", req.level.to_string())
        .text("status", req.status.as_form_str().to_string())
        .text("save", if req.save { "true" } else { "false" }.to_string());
    if !req.tags.is_empty() {
        form = form.text("tags", req.tags.join(","));
    }
    if let Some(audio_path) = req.audio {
        let bytes = tokio::fs::read(audio_path)
            .await
            .map_err(|e| LingqError::Io(e.to_string()))?;
        let name = audio_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.mp3")
            .to_string();
        let part = Part::bytes(bytes)
            .file_name(name)
            .mime_str("audio/mpeg")
            .map_err(|e| LingqError::Transport(e.to_string()))?;
        form = form.part("audio", part);
    }
    Ok(form)
}

async fn parse_lesson_id(resp: reqwest::Response) -> Result<i64, LingqError> {
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LingqError::Transport(e.to_string()))?;
    v.get("pk")
        .or_else(|| v.get("id"))
        .and_then(|x| x.as_i64())
        .ok_or_else(|| LingqError::Schema("import_lesson response missing pk/id".into()))
}
