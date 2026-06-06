use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use unicode_normalization::UnicodeNormalization;

use super::client::LingqClient;
use super::collections::CollectionId;
use super::error::LingqError;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct LessonSummary {
    pub id: i64,
    pub title: String,
}

impl LingqClient {
    /// Paginate `/api/v3/{lang}/collections/{cid}/lessons/` and collect summaries.
    pub async fn list_lessons(
        &self,
        cid: CollectionId,
    ) -> Result<Vec<LessonSummary>, LingqError> {
        let mut out: Vec<LessonSummary> = Vec::new();
        let mut page = 1;
        loop {
            let url = format!(
                "{}/api/v3/{}/collections/{}/lessons/?page={}&page_size=100",
                self.base_url(),
                self.lang(),
                cid.0,
                page
            );
            let resp = self
                .http()
                .get(&url)
                .header("Authorization", self.auth_header())
                .send()
                .await
                .map_err(|e| LingqError::Transport(e.to_string()))?;
            match resp.status() {
                s if s.is_success() => {
                    let body: serde_json::Value = resp
                        .json()
                        .await
                        .map_err(|e| LingqError::Transport(e.to_string()))?;
                    let results = body
                        .get("results")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .or_else(|| body.as_array().cloned())
                        .unwrap_or_default();
                    if results.is_empty() {
                        break;
                    }
                    for v in &results {
                        let id = v
                            .get("pk")
                            .or_else(|| v.get("id"))
                            .and_then(|x| x.as_i64());
                        let title = v
                            .get("title")
                            .and_then(|x| x.as_str())
                            .map(str::to_string);
                        if let (Some(id), Some(title)) = (id, title) {
                            out.push(LessonSummary { id, title });
                        }
                    }
                    if body.get("next").map(|v| v.is_null()).unwrap_or(true) {
                        break;
                    }
                    page += 1;
                }
                StatusCode::UNAUTHORIZED => return Err(LingqError::Unauthorized),
                s if s.is_client_error() => {
                    let detail = resp.text().await.unwrap_or_default();
                    return Err(LingqError::BadRequest(detail));
                }
                s if s.is_server_error() => {
                    let detail = resp.text().await.unwrap_or_default();
                    return Err(LingqError::Server(detail));
                }
                other => {
                    return Err(LingqError::Transport(format!(
                        "unexpected status {other}"
                    )))
                }
            }
        }
        Ok(out)
    }
}

/// SHA-256 of NFC-normalised, lower-cased, full→half-width folded title.
pub fn title_hash(title: &str) -> [u8; 32] {
    let folded = fold_half_width(title);
    let nfc: String = folded.nfc().collect();
    let lower = nfc.to_lowercase();
    let mut h = Sha256::new();
    h.update(lower.as_bytes());
    h.finalize().into()
}

fn fold_half_width(s: &str) -> String {
    s.chars()
        .map(|c| match c as u32 {
            // Full-width ASCII -> half-width.
            0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            0x3000 => ' ',
            _ => c,
        })
        .collect()
}

/// Filter `local` titles, dropping any that appear in `remote` (by `title_hash`).
pub fn dedup<'a>(local: &'a [&'a str], remote: &[LessonSummary]) -> Vec<&'a str> {
    let remote_hashes: std::collections::HashSet<[u8; 32]> =
        remote.iter().map(|l| title_hash(&l.title)).collect();
    local
        .iter()
        .copied()
        .filter(|t| !remote_hashes.contains(&title_hash(t)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_hash_nfc_stable() {
        // U+30AB KATAKANA KA + U+3099 voicing == precomposed U+30AC.
        let a = title_hash("ガ");
        let b = title_hash("\u{30AB}\u{3099}");
        assert_eq!(a, b);
    }

    #[test]
    fn title_hash_full_to_half_width() {
        let a = title_hash("Chapter1");
        let b = title_hash("Ｃｈａｐｔｅｒ１");
        assert_eq!(a, b);
    }

    #[test]
    fn dedup_drops_already_present_titles() {
        let local = ["Chapter 1", "Chapter 2", "Chapter 3"];
        let local: Vec<&str> = local.to_vec();
        let remote = vec![LessonSummary {
            id: 100,
            title: "Chapter 2".into(),
        }];
        let out = dedup(&local, &remote);
        assert_eq!(out, vec!["Chapter 1", "Chapter 3"]);
    }
}
