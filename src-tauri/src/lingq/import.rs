use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    pub level: u8,
    pub status: LessonStatus,
    pub tags: &'a [&'a str],
    pub save: bool,
}

const MAX_ATTEMPTS: u32 = 3;

impl LingqClient {
    /// Import a single lesson with full multipart fields. Language comes from
    /// the client (AD-017) — both URL segment and multipart `language` field
    /// derive from `self.lang()`. Retries on 5xx with capped, jittered
    /// exponential backoff (max 3 attempts); 4xx fails fast.
    pub async fn import_lesson_v2(&self, req: ImportLessonRequest<'_>) -> Result<i64, LingqError> {
        let url = format!(
            "{}/api/v3/{}/lessons/import/",
            self.base_url(),
            self.lang()
        );
        // Read audio once; multipart Part::bytes accepts owned Vec, so we clone
        // per attempt instead of re-reading from disk.
        let audio_bytes = match req.audio {
            Some(path) => Some(
                tokio::fs::read(path)
                    .await
                    .map_err(|e| LingqError::Io(e.to_string()))?,
            ),
            None => None,
        };
        let audio_name = req.audio.map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("audio.mp3")
                .to_string()
        });

        let mut last_err: Option<LingqError> = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(backoff(attempt)).await;
            }
            let form = build_form(&req, self.lang(), audio_bytes.as_deref(), audio_name.as_deref())?;
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
                    last_err = Some(LingqError::Transport(format!("unexpected status {other}")))
                }
            }
        }
        Err(last_err.expect("retry loop ran at least once"))
    }
}

/// 200ms * 2^attempt, +/-25% jitter. Pseudo-random based on nanosecond clock —
/// good enough to desynchronise thundering retries; not crypto.
fn backoff(attempt: u32) -> Duration {
    let base_ms = 200u64.checked_shl(attempt).unwrap_or(u64::MAX);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // jitter in [-0.25, 0.25] of base_ms
    let r = (nanos as f64) / (u32::MAX as f64); // in [0, 1]
    let jitter = (r - 0.5) * 0.5; // in [-0.25, 0.25]
    let scaled = (base_ms as f64 * (1.0 + jitter)).max(0.0) as u64;
    Duration::from_millis(scaled)
}

fn build_form(
    req: &ImportLessonRequest<'_>,
    lang: &str,
    audio_bytes: Option<&[u8]>,
    audio_name: Option<&str>,
) -> Result<Form, LingqError> {
    let mut form = Form::new()
        .text("title", req.title.to_string())
        .text("text", req.text.to_string())
        .text("collection", req.collection.0.to_string())
        .text("language", lang.to_string())
        .text("level", req.level.to_string())
        .text("status", req.status.as_form_str().to_string())
        .text("save", if req.save { "true" } else { "false" }.to_string());
    if !req.tags.is_empty() {
        form = form.text("tags", req.tags.join(","));
    }
    if let (Some(bytes), Some(name)) = (audio_bytes, audio_name) {
        let part = Part::bytes(bytes.to_vec())
            .file_name(name.to_string())
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
