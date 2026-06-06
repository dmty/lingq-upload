use std::fmt;
use std::path::Path;

use reqwest::multipart::{Form, Part};
use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::lingq::error::LingqError;

const DEFAULT_BASE_URL: &str = "https://www.lingq.com";

pub struct LingqClient {
    http: Client,
    api_key: SecretString,
    lang: String,
    base_url: String,
}

impl fmt::Debug for LingqClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LingqClient")
            .field("lang", &self.lang)
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct WhoAmI {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LessonOpts {
    pub level: String,
    pub status: String,
    pub tags: String,
    pub save: String,
}

impl Default for LessonOpts {
    fn default() -> Self {
        Self {
            level: "1".into(),
            status: "private".into(),
            tags: "books".into(),
            save: "true".into(),
        }
    }
}

impl LingqClient {
    pub fn new(api_key: SecretString, lang: impl Into<String>) -> Self {
        Self::with_base_url(api_key, lang, DEFAULT_BASE_URL.to_string())
    }

    pub fn with_base_url(
        api_key: SecretString,
        lang: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        let http = Client::builder()
            .user_agent(concat!("lingq-upload/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client build");
        Self {
            http,
            api_key,
            lang: lang.into(),
            base_url: base_url.into(),
        }
    }

    fn auth_header(&self) -> String {
        format!("Token {}", self.api_key.expose_secret())
    }

    pub async fn whoami(&self) -> Result<WhoAmI, LingqError> {
        let url = format!(
            "{}/api/v3/{}/collections/my/?page_size=1",
            self.base_url, self.lang
        );
        tracing::debug!(lang = %self.lang, "lingq whoami");

        let resp = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| LingqError::Transport(e.to_string()))?;

        let status = resp.status();
        tracing::debug!(status = %status, "lingq whoami response");

        match status {
            StatusCode::OK => Ok(WhoAmI { ok: true }),
            StatusCode::UNAUTHORIZED => Err(LingqError::Unauthorized),
            StatusCode::NOT_FOUND => Err(LingqError::NotFound),
            s if s.is_client_error() => Err(LingqError::BadRequest(read_detail(resp).await)),
            s if s.is_server_error() => Err(LingqError::Server(read_detail(resp).await)),
            other => Err(LingqError::Transport(format!("unexpected status {other}"))),
        }
    }

    pub async fn import_lesson(
        &self,
        collection_id: i64,
        title: &str,
        text: &str,
        audio_mp3: &Path,
        opts: &LessonOpts,
    ) -> Result<i64, LingqError> {
        let url = format!("{}/api/v3/{}/lessons/import/", self.base_url, self.lang);

        let audio_bytes = tokio::fs::read(audio_mp3)
            .await
            .map_err(|e| LingqError::Io(e.to_string()))?;
        let audio_name = audio_mp3
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.mp3")
            .to_string();

        let audio_part = Part::bytes(audio_bytes)
            .file_name(audio_name)
            .mime_str("audio/mpeg")
            .map_err(|e| LingqError::Transport(e.to_string()))?;

        let form = Form::new()
            .text("title", title.to_string())
            .text("text", text.to_string())
            .text("collection", collection_id.to_string())
            .text("language", self.lang.clone())
            .text("level", opts.level.clone())
            .text("status", opts.status.clone())
            .text("tags", opts.tags.clone())
            .text("save", opts.save.clone())
            .part("audio", audio_part);

        tracing::debug!(lang = %self.lang, collection = collection_id, "lingq import_lesson");

        let resp = self
            .http
            .post(&url)
            .header("Authorization", self.auth_header())
            .multipart(form)
            .send()
            .await
            .map_err(|e| LingqError::Transport(e.to_string()))?;

        let status = resp.status();
        tracing::debug!(status = %status, "lingq import_lesson response");

        match status {
            s if s.is_success() => parse_lesson_id(resp).await,
            StatusCode::UNAUTHORIZED => Err(LingqError::Unauthorized),
            StatusCode::NOT_FOUND => Err(LingqError::NotFound),
            s if s.is_client_error() => Err(LingqError::BadRequest(read_detail(resp).await)),
            s if s.is_server_error() => Err(LingqError::Server(read_detail(resp).await)),
            other => Err(LingqError::Transport(format!("unexpected status {other}"))),
        }
    }
}

async fn parse_lesson_id(resp: reqwest::Response) -> Result<i64, LingqError> {
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LingqError::Transport(e.to_string()))?;
    let id = body
        .get("id")
        .ok_or_else(|| LingqError::Schema("missing `id` in import_lesson response".into()))?;
    id.as_i64()
        .ok_or_else(|| LingqError::Schema(format!("`id` is not an integer: {id}")))
}

async fn read_detail(resp: reqwest::Response) -> String {
    match resp.text().await {
        Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(v) => v
                .get("detail")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string())
                .unwrap_or(body),
            Err(_) => body,
        },
        Err(e) => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_api_key() {
        let c = LingqClient::new(SecretString::from("super-secret-token".to_string()), "ja");
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("super-secret-token"));
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn default_opts_match_spec() {
        let o = LessonOpts::default();
        assert_eq!(o.level, "1");
        assert_eq!(o.status, "private");
        assert_eq!(o.tags, "books");
        assert_eq!(o.save, "true");
    }
}
