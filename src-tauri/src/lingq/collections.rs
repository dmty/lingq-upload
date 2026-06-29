use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::multipart::{Form, Part};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

use super::client::LingqClient;
use super::error::LingqError;
use super::lessons::title_hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct CollectionId(pub i64);

type LockKey = (String, [u8; 32]);
type LockMap = Mutex<HashMap<LockKey, Arc<AsyncMutex<()>>>>;

/// In-process serialiser for `find_or_create_collection` keyed by `(lang, title_hash)`.
/// Prevents two concurrent calls within the same process from both POSTing when the
/// server has no existing match. Does NOT protect across multiple app instances —
/// that case relies on the post-POST re-search fallback below + server-side dedupe.
fn create_locks() -> &'static LockMap {
    static LOCKS: OnceLock<LockMap> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_for(lang: &str, title: &str) -> Arc<AsyncMutex<()>> {
    let key = (lang.to_string(), title_hash(title));
    let mut map = create_locks().lock().expect("collection locks poisoned");
    map.entry(key)
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

impl LingqClient {
    /// Find an existing collection by normalised-title equality or create one.
    ///
    /// Title comparison uses [`title_hash`] (NFC + half-width fold + whitespace
    /// collapse + lowercase) so that NFC/NFD variants, full/half-width digits,
    /// and incidental whitespace do not produce duplicates.
    ///
    /// Concurrency: serialised in-process per `(lang, title)` via an async mutex.
    /// If the search-then-create race still slips through (different process,
    /// different machine), a 4xx on the POST triggers a single re-search and the
    /// found id is returned. Only if the re-search also misses do we surface
    /// `BadRequest`.
    pub async fn find_or_create_collection(
        &self,
        title: &str,
        description: &str,
    ) -> Result<CollectionId, LingqError> {
        let guard = lock_for(self.lang(), title);
        let _held = guard.lock().await;

        if let Some(id) = self.search_collection(title).await? {
            return Ok(id);
        }

        let create_url = format!("{}/api/v3/{}/collections/", self.base_url(), self.lang());
        let body = serde_json::json!({
            "title": title,
            "description": description,
            "tags": ["books"],
        });
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
                // Race-after-search: another writer may have just created it.
                match self.search_collection(title).await? {
                    Some(id) => Ok(id),
                    None => Err(LingqError::BadRequest(detail)),
                }
            }
            s if s.is_server_error() => {
                let detail = resp.text().await.unwrap_or_default();
                Err(LingqError::Server(detail))
            }
            other => Err(LingqError::Transport(format!("unexpected status {other}"))),
        }
    }

    async fn search_collection(&self, title: &str) -> Result<Option<CollectionId>, LingqError> {
        let search_url = format!(
            "{}/api/v3/{}/collections/?search={}",
            self.base_url(),
            self.lang(),
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
                let target = title_hash(title);
                for v in &items {
                    let t = v.get("title").and_then(|x| x.as_str()).unwrap_or("");
                    if title_hash(t) == target {
                        if let Some(id) =
                            v.get("pk").or_else(|| v.get("id")).and_then(|x| x.as_i64())
                        {
                            return Ok(Some(CollectionId(id)));
                        }
                    }
                }
                Ok(None)
            }
            StatusCode::UNAUTHORIZED => Err(LingqError::Unauthorized),
            _ => Ok(None),
        }
    }
}

/// Probe candidate for the cover-image PATCH. The cascade tries each in
/// order; the first 2xx wins and is cached for the lifetime of the process.
#[derive(Debug, Clone, Copy)]
enum CoverProbe {
    /// PATCH /api/v3/{lang}/collections/{cid}/image/ with multipart `image`.
    DedicatedImageEndpoint,
    /// PATCH /api/v3/{lang}/collections/{cid}/ with multipart `image`.
    CollectionImageField,
    /// PATCH /api/v3/{lang}/collections/{cid}/ with multipart `cover`.
    CollectionCoverField,
}

impl CoverProbe {
    const ALL: [CoverProbe; 3] = [
        CoverProbe::DedicatedImageEndpoint,
        CoverProbe::CollectionImageField,
        CoverProbe::CollectionCoverField,
    ];

    fn url(self, base: &str, lang: &str, cid: i64) -> String {
        match self {
            CoverProbe::DedicatedImageEndpoint => {
                format!("{base}/api/v3/{lang}/collections/{cid}/image/")
            }
            CoverProbe::CollectionImageField | CoverProbe::CollectionCoverField => {
                format!("{base}/api/v3/{lang}/collections/{cid}/")
            }
        }
    }

    fn field_name(self) -> &'static str {
        match self {
            CoverProbe::DedicatedImageEndpoint | CoverProbe::CollectionImageField => "image",
            CoverProbe::CollectionCoverField => "cover",
        }
    }

    fn label(self) -> &'static str {
        match self {
            CoverProbe::DedicatedImageEndpoint => "dedicated-image-endpoint",
            CoverProbe::CollectionImageField => "collection-image-field",
            CoverProbe::CollectionCoverField => "collection-cover-field",
        }
    }
}

static WINNER: OnceLock<CoverProbe> = OnceLock::new();

fn mime_for_path(p: &Path) -> &'static str {
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

impl LingqClient {
    /// Upload `img` as the cover image for `cid`. Returns `Ok(true)` on the
    /// first probe that yields 2xx (cached for the process lifetime). Returns
    /// `Ok(false)` when all probes exhaust with 4xx — caller should log and
    /// continue. Returns `Err` on 5xx or transport errors.
    // SIMPLIFY: no retry on cover 5xx; surface to caller (soft-fail). Add
    // retry if telemetry shows transient 5xx is common.
    pub async fn set_collection_image(
        &self,
        cid: CollectionId,
        img: &Path,
    ) -> Result<bool, LingqError> {
        let bytes = tokio::fs::read(img)
            .await
            .map_err(|e| LingqError::Io(e.to_string()))?;
        let file_name = img
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("cover.jpg")
            .to_string();
        let mime = mime_for_path(img);

        let probes: &[CoverProbe] = match WINNER.get() {
            Some(w) => std::slice::from_ref(w),
            None => &CoverProbe::ALL,
        };

        for probe in probes {
            let url = probe.url(self.base_url(), self.lang(), cid.0);
            let part = Part::bytes(bytes.clone())
                .file_name(file_name.clone())
                .mime_str(mime)
                .map_err(|e| LingqError::Transport(e.to_string()))?;
            let form = Form::new().part(probe.field_name(), part);

            let resp = self
                .http()
                .patch(&url)
                .header("Authorization", self.auth_header())
                .multipart(form)
                .send()
                .await
                .map_err(|e| LingqError::Transport(e.to_string()))?;
            let status = resp.status();
            if status.is_success() {
                let _ = WINNER.set(*probe);
                tracing::info!(probe = probe.label(), "lingq cover upload succeeded");
                return Ok(true);
            }
            if status.is_client_error() {
                continue;
            }
            return Err(LingqError::Server(format!("PATCH {url} → {status}")));
        }
        Ok(false)
    }
}

fn urlencode(s: &str) -> String {
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
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
