use std::fmt;
use std::path::Path;

use reqwest::multipart::{Form, Part};
use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::lingq::error::LingqError;
use crate::lingq::lang::LanguageCode;

const DEFAULT_BASE_URL: &str = "https://www.lingq.com";

pub struct LingqClient {
    http: Client,
    api_key: SecretString,
    lang: LanguageCode,
    base_url: String,
}

impl fmt::Debug for LingqClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LingqClient")
            .field("lang", &self.lang.as_str())
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
pub struct Language {
    pub code: String,
    pub title: String,
    pub known_words: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Collection {
    pub id: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AccountProfile {
    pub username: String,
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
    pub fn new(api_key: SecretString, lang: LanguageCode) -> Self {
        Self::with_base_url(api_key, lang, DEFAULT_BASE_URL.to_string())
    }

    pub fn with_base_url(
        api_key: SecretString,
        lang: LanguageCode,
        base_url: impl Into<String>,
    ) -> Self {
        let http = Client::builder()
            .user_agent(concat!("lingq-upload/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client build");
        Self {
            http,
            api_key,
            lang,
            base_url: base_url.into(),
        }
    }

    pub(crate) fn auth_header(&self) -> String {
        format!("Token {}", self.api_key.expose_secret())
    }

    pub(crate) fn http(&self) -> &Client {
        &self.http
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) fn lang(&self) -> &str {
        self.lang.as_str()
    }

    /// Fetch the caller's account profile. Tries the known candidates in order:
    /// `/api/v2/api-profile/`, `/api/v2/profile/`, `/api/v3/api-profile/`.
    /// The browser extension uses the username it returns to scope follow-up
    /// calls like `/api/v2/languages/?username=…`.
    pub async fn account_profile(&self) -> Result<AccountProfile, LingqError> {
        let candidates = [
            "/api/v2/api-profile/",
            "/api/v2/profile/",
            "/api/v3/api-profile/",
        ];
        let mut last_err: Option<LingqError> = None;
        for path in candidates {
            let url = format!("{}{}", self.base_url, path);
            tracing::debug!(path, "lingq account_profile probe");
            let resp = match self
                .http
                .get(&url)
                .header("Authorization", self.auth_header())
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(LingqError::Transport(e.to_string()));
                    continue;
                }
            };
            let status = resp.status();
            tracing::debug!(path, status = %status, "lingq account_profile response");
            if status == StatusCode::UNAUTHORIZED {
                return Err(LingqError::Unauthorized);
            }
            if !status.is_success() {
                last_err = Some(if status.is_client_error() {
                    LingqError::BadRequest(read_detail(resp).await)
                } else {
                    LingqError::Server(read_detail(resp).await)
                });
                continue;
            }
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| LingqError::Transport(e.to_string()))?;
            if let Some(username) =
                pick_str(&body, &["username", "user_name", "user"]).map(str::to_string)
            {
                return Ok(AccountProfile { username });
            }
            last_err = Some(LingqError::Schema(format!(
                "{path}: response missing recognisable username field"
            )));
        }
        Err(last_err.unwrap_or_else(|| {
            LingqError::Schema("no profile endpoint candidate succeeded".into())
        }))
    }

    pub async fn list_my_languages(&self) -> Result<Vec<Language>, LingqError> {
        self.list_languages_inner(None).await
    }

    /// User-scoped variant. With a username, LingQ trims the catalogue down to
    /// the user's enrolled languages (matches the browser extension's behaviour).
    pub async fn list_my_languages_for(&self, username: &str) -> Result<Vec<Language>, LingqError> {
        self.list_languages_inner(Some(username)).await
    }

    async fn list_languages_inner(
        &self,
        username: Option<&str>,
    ) -> Result<Vec<Language>, LingqError> {
        // /api/v2/languages/ is one of two surviving v2 endpoints — it returns the
        // catalogue plus the caller's known-word counts. Not lang-scoped.
        let url = match username {
            Some(u) => format!(
                "{}/api/v2/languages/?username={}",
                self.base_url,
                urlencode(u)
            ),
            None => format!("{}/api/v2/languages/", self.base_url),
        };
        tracing::debug!(scoped = username.is_some(), "lingq list_my_languages");

        let resp = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| LingqError::Transport(e.to_string()))?;

        let status = resp.status();
        tracing::debug!(status = %status, "lingq list_my_languages response");

        match status {
            s if s.is_success() => parse_languages(resp).await,
            StatusCode::UNAUTHORIZED => Err(LingqError::Unauthorized),
            StatusCode::NOT_FOUND => Err(LingqError::NotFound),
            s if s.is_client_error() => Err(LingqError::BadRequest(read_detail(resp).await)),
            s if s.is_server_error() => Err(LingqError::Server(read_detail(resp).await)),
            other => Err(LingqError::Transport(format!("unexpected status {other}"))),
        }
    }

    pub async fn list_my_collections(&self) -> Result<Vec<Collection>, LingqError> {
        let url = format!(
            "{}/api/v3/{}/collections/my/?page_size=200",
            self.base_url,
            self.lang()
        );
        tracing::debug!(lang = %self.lang(), "lingq list_my_collections");

        let resp = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| LingqError::Transport(e.to_string()))?;

        let status = resp.status();
        tracing::debug!(status = %status, "lingq list_my_collections response");

        match status {
            s if s.is_success() => parse_collections(resp).await,
            StatusCode::UNAUTHORIZED => Err(LingqError::Unauthorized),
            StatusCode::NOT_FOUND => Err(LingqError::NotFound),
            s if s.is_client_error() => Err(LingqError::BadRequest(read_detail(resp).await)),
            s if s.is_server_error() => Err(LingqError::Server(read_detail(resp).await)),
            other => Err(LingqError::Transport(format!("unexpected status {other}"))),
        }
    }

    pub async fn whoami(&self) -> Result<WhoAmI, LingqError> {
        let url = format!(
            "{}/api/v3/{}/collections/my/?page_size=1",
            self.base_url,
            self.lang()
        );
        tracing::debug!(lang = %self.lang(), "lingq whoami");

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
        let url = format!(
            "{}/api/v3/{}/lessons/import/",
            self.base_url,
            self.lang()
        );

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
            .text("language", self.lang().to_string())
            .text("level", opts.level.clone())
            .text("status", opts.status.clone())
            .text("tags", opts.tags.clone())
            .text("save", opts.save.clone())
            .part("audio", audio_part);

        tracing::debug!(lang = %self.lang(), collection = collection_id, "lingq import_lesson");

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

fn urlencode(s: &str) -> String {
    // Minimal RFC 3986 percent-encoding for query values. Avoids pulling a crate
    // just for usernames — only need to handle the unreserved/reserved split.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn pick_str<'a>(v: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(k).and_then(|x| x.as_str()))
}

fn pick_i64(v: &serde_json::Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|k| v.get(k).and_then(|x| x.as_i64()))
}

async fn read_items(resp: reqwest::Response) -> Result<Vec<serde_json::Value>, LingqError> {
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LingqError::Transport(e.to_string()))?;
    if let Some(arr) = body.as_array() {
        return Ok(arr.clone());
    }
    if let Some(arr) = body.get("results").and_then(|v| v.as_array()) {
        return Ok(arr.clone());
    }
    Err(LingqError::Schema(
        "expected JSON array or object with `results: []`".into(),
    ))
}

async fn parse_languages(resp: reqwest::Response) -> Result<Vec<Language>, LingqError> {
    let items = read_items(resp).await?;
    let mut out = Vec::with_capacity(items.len());
    for v in &items {
        // LingQ surfaces the language slug under varying names across endpoints;
        // accept the common ones and skip rows missing both code + title.
        let code = pick_str(v, &["code", "language", "url_slug", "tag"]).map(str::to_string);
        let title = pick_str(v, &["title", "english_name", "name", "label"]).map(str::to_string);
        let known_words = pick_i64(
            v,
            &["known_words", "knownWords", "words_known", "wordsKnown"],
        )
        .unwrap_or(0);
        if let (Some(code), Some(title)) = (code, title) {
            out.push(Language {
                code,
                title,
                known_words,
            });
        }
    }
    if out.is_empty() && !items.is_empty() {
        return Err(LingqError::Schema(
            "languages payload had entries but none had a recognisable code+title pair".into(),
        ));
    }
    Ok(out)
}

async fn parse_collections(resp: reqwest::Response) -> Result<Vec<Collection>, LingqError> {
    let items = read_items(resp).await?;
    let mut out = Vec::with_capacity(items.len());
    for v in &items {
        let id = pick_i64(v, &["id", "pk"]);
        let title = pick_str(v, &["title", "name"]).map(str::to_string);
        if let (Some(id), Some(title)) = (id, title) {
            out.push(Collection { id, title });
        }
    }
    Ok(out)
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
        let c = LingqClient::new(
            SecretString::from("super-secret-token".to_string()),
            LanguageCode::new("ja").expect("valid lang"),
        );
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
