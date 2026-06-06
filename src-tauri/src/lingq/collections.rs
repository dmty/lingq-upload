use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::client::LingqClient;
use super::error::LingqError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct CollectionId(pub i64);

impl LingqClient {
    /// Find an existing collection by exact title or create a new one.
    /// Idempotent against concurrent calls: a client-side mutex over
    /// (lang, title) is the caller's responsibility (one per app instance).
    pub async fn find_or_create_collection(
        &self,
        title: &str,
        description: &str,
        lang: &str,
    ) -> Result<CollectionId, LingqError> {
        let search_url = format!(
            "{}/api/v3/{}/collections/?search={}",
            self.base_url(),
            lang,
            urlencode(title)
        );
        let resp = self
            .http()
            .get(&search_url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| LingqError::Transport(e.to_string()))?;
        match resp.status() {
            s if s.is_success() => {
                let items = read_items(resp).await?;
                for v in &items {
                    let t = v.get("title").and_then(|x| x.as_str()).unwrap_or("");
                    if t == title {
                        if let Some(id) = v
                            .get("pk")
                            .or_else(|| v.get("id"))
                            .and_then(|x| x.as_i64())
                        {
                            return Ok(CollectionId(id));
                        }
                    }
                }
            }
            StatusCode::UNAUTHORIZED => return Err(LingqError::Unauthorized),
            _ => {}
        }

        let create_url = format!("{}/api/v3/{}/collections/", self.base_url(), lang);
        let body = serde_json::json!({ "title": title, "description": description });
        let resp = self
            .http()
            .post(&create_url)
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| LingqError::Transport(e.to_string()))?;
        match resp.status() {
            s if s.is_success() => {
                let v: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| LingqError::Transport(e.to_string()))?;
                v.get("pk")
                    .or_else(|| v.get("id"))
                    .and_then(|x| x.as_i64())
                    .map(CollectionId)
                    .ok_or_else(|| LingqError::Schema("collection id missing".into()))
            }
            StatusCode::UNAUTHORIZED => Err(LingqError::Unauthorized),
            s if s.is_client_error() => {
                let detail = resp.text().await.unwrap_or_default();
                Err(LingqError::BadRequest(detail))
            }
            s if s.is_server_error() => {
                let detail = resp.text().await.unwrap_or_default();
                Err(LingqError::Server(detail))
            }
            other => Err(LingqError::Transport(format!("unexpected status {other}"))),
        }
    }
}

fn urlencode(s: &str) -> String {
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
        "expected JSON array or `results: []`".into(),
    ))
}
