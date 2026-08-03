use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::client::LingqClient;
use super::collections::CollectionId;
use super::error::LingqError;

/// Course-screen projection of `GET /collections/{cid}/`.
///
/// Every field except `id`/`title` is optional: the LingQ v3 surface is
/// observed rather than documented (`docs/specs/lingq-api.md`) and changes
/// silently. A renamed field must degrade to a blank cell, never to a parse
/// error that blanks the whole screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct CollectionDetail {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub level: Option<String>,
    pub difficulty: Option<f64>,
    pub duration: Option<i64>,
    pub lessons_count: Option<i64>,
    pub new_words_count: Option<i64>,
    pub image_url: Option<String>,
    pub status: Option<String>,
    pub roses_count: Option<i64>,
    pub views_count: Option<i64>,
}

pub(crate) fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn i64_field(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_i64())
}

fn parse_collection_detail(v: &serde_json::Value) -> Result<CollectionDetail, LingqError> {
    let id = v
        .get("pk")
        .or_else(|| v.get("id"))
        .and_then(|x| x.as_i64())
        .ok_or_else(|| LingqError::Schema("collection detail missing id".into()))?;
    Ok(CollectionDetail {
        id,
        title: str_field(v, "title").unwrap_or_default(),
        description: str_field(v, "description"),
        level: str_field(v, "level"),
        difficulty: v.get("difficulty").and_then(|x| x.as_f64()),
        duration: i64_field(v, "duration"),
        lessons_count: i64_field(v, "lessonsCount"),
        new_words_count: i64_field(v, "newWordsCount"),
        image_url: str_field(v, "imageUrl"),
        status: str_field(v, "status"),
        roses_count: i64_field(v, "rosesCount"),
        views_count: i64_field(v, "viewsCount"),
    })
}

impl LingqClient {
    pub async fn collection_detail(
        &self,
        cid: CollectionId,
    ) -> Result<CollectionDetail, LingqError> {
        let url = format!(
            "{}/api/v3/{}/collections/{}/",
            self.base_url(),
            self.lang(),
            cid.0
        );
        tracing::debug!(lang = %self.lang(), collection = cid.0, "lingq collection_detail");

        let resp = self
            .http()
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| LingqError::Transport(e.to_string()))?;

        let status = resp.status();
        tracing::debug!(status = %status, "lingq collection_detail response");

        match status {
            s if s.is_success() => {
                let body: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| LingqError::Transport(e.to_string()))?;
                parse_collection_detail(&body)
            }
            StatusCode::UNAUTHORIZED => Err(LingqError::Unauthorized),
            StatusCode::NOT_FOUND => Err(LingqError::NotFound),
            s if s.is_client_error() => Err(LingqError::BadRequest(
                resp.text().await.unwrap_or_default(),
            )),
            s if s.is_server_error() => {
                Err(LingqError::Server(resp.text().await.unwrap_or_default()))
            }
            other => Err(LingqError::Transport(format!("unexpected status {other}"))),
        }
    }
}
