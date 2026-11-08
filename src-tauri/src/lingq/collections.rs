use std::path::Path;
use std::sync::OnceLock;

use reqwest::multipart::{Form, Part};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::client::{read_detail, LingqClient};
use super::error::LingqError;
use super::lessons::title_hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct CollectionId(pub i64);

impl LingqClient {
    /// Always creates a new collection. Reusing an existing one is the
    /// caller's decision: see [`Self::find_my_collection`].
    pub async fn create_collection(&self, title: &str) -> Result<CollectionId, LingqError> {
        let create_url = format!("{}/api/v3/{}/collections/", self.base_url(), self.lang());
        let body = serde_json::json!({
            "title": title,
            "description": "",
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
            s if s.is_client_error() => Err(LingqError::BadRequest(read_detail(resp).await)),
            s if s.is_server_error() => Err(LingqError::Server(read_detail(resp).await)),
            other => Err(LingqError::Transport(format!("unexpected status {other}"))),
        }
    }

    /// The user's own collection whose title matches under [`title_hash`],
    /// walking every page.
    ///
    /// LingQ has no usable title filter: `/collections/?search=` ignores the
    /// parameter, and `/collections/my/?search=` only hits byte-exact titles,
    /// so an NFC request misses a course stored NFD.
    pub async fn find_my_collection(
        &self,
        title: &str,
    ) -> Result<Option<CollectionId>, LingqError> {
        let target = title_hash(title);
        let mut page = 1;
        loop {
            let url = format!(
                "{}/api/v3/{}/collections/my/?page_size=200&page={page}",
                self.base_url(),
                self.lang(),
            );
            let resp = self
                .http()
                .get(&url)
                .header("Authorization", self.auth_header())
                .send()
                .await
                .map_err(|e| LingqError::Transport(e.to_string()))?;
            let body: serde_json::Value = match resp.status() {
                s if s.is_success() => resp
                    .json()
                    .await
                    .map_err(|e| LingqError::Transport(e.to_string()))?,
                StatusCode::UNAUTHORIZED => return Err(LingqError::Unauthorized),
                s if s.is_client_error() => {
                    return Err(LingqError::BadRequest(read_detail(resp).await));
                }
                _ => return Err(LingqError::Server(read_detail(resp).await)),
            };
            let items = body
                .get("results")
                .and_then(|v| v.as_array())
                .ok_or_else(|| LingqError::Schema("expected `results: []`".into()))?;
            let hit = items.iter().find_map(|v| {
                let t = v.get("title").and_then(|x| x.as_str())?;
                (title_hash(t) == target)
                    .then(|| v.get("id").or_else(|| v.get("pk")).and_then(|x| x.as_i64()))
                    .flatten()
            });
            if let Some(id) = hit {
                return Ok(Some(CollectionId(id)));
            }
            if body.get("next").is_none_or(|n| n.is_null()) {
                return Ok(None);
            }
            page += 1;
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
    /// continue. Retries 5xx with the same backoff as lesson import, then
    /// `Err`; transport errors fail immediately.
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

        'probes: for probe in probes {
            let url = probe.url(self.base_url(), self.lang(), cid.0);
            let mut last_5xx = None;
            for attempt in 0..super::import::MAX_ATTEMPTS {
                if attempt > 0 {
                    tokio::time::sleep(super::import::backoff(attempt)).await;
                }
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
                    continue 'probes;
                }
                last_5xx = Some(LingqError::Server(format!("PATCH {url} → {status}")));
            }
            return Err(last_5xx.expect("retry loop ran at least once"));
        }
        Ok(false)
    }
}
