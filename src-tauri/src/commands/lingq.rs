use std::sync::Arc;

use secrecy::SecretString;
use tauri::AppHandle;

use super::{app_data_dir, parse_lang};
use crate::commands::project::set_cover_bytes_impl;
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::lingq::{
    AccountProfile, Collection, CollectionId, CourseView, Language, LanguageCode, LingqClient,
};
use crate::secrets::{SecretsStore, LINGQ_ACCOUNT};

const MAX_COVER_BYTES: usize = 5 * 1024 * 1024;

fn load_api_key(app: &AppHandle) -> Result<String, AppError> {
    let store = SecretsStore::new_default(&app_data_dir(app)?, LINGQ_ACCOUNT);
    store.load_key()?.ok_or(AppError::MissingApiKey)
}

fn client_for(app: &AppHandle, lang: LanguageCode) -> Result<LingqClient, AppError> {
    let key = load_api_key(app)?;
    Ok(LingqClient::new(SecretString::from(key), lang))
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_account_profile(app: AppHandle) -> Result<AccountProfile, AppError> {
    // /api/v2/api-profile/ et al. are not lang-scoped; the segment is a placeholder.
    let client = client_for(&app, parse_lang("en")?)?;
    Ok(client.account_profile().await?)
}

/// Returns the caller's enrolled languages. With a username we get the
/// user-trimmed catalogue (matches the browser extension); without one we
/// fall back to the full catalogue and let the UI filter by known_words.
#[tauri::command]
#[specta::specta]
pub async fn cmd_list_languages(
    app: AppHandle,
    username: Option<String>,
) -> Result<Vec<Language>, AppError> {
    let client = client_for(&app, parse_lang("en")?)?;
    let langs = match username.as_deref() {
        Some(u) if !u.is_empty() => client.list_my_languages_for(u).await?,
        _ => client.list_my_languages().await?,
    };
    Ok(langs)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_list_collections(
    app: AppHandle,
    lang: String,
) -> Result<Vec<Collection>, AppError> {
    let client = client_for(&app, parse_lang(&lang)?)?;
    Ok(client.list_my_collections().await?)
}

/// Course-screen payload: collection header stats plus per-lesson stats.
///
/// Two requests, never a per-lesson fetch. `lang` comes from the project entry —
/// cross-language calls 404 (AD-017).
///
/// When the matching project has no local cover and LingQ returned `imageUrl`,
/// the bytes are copied into the project dir. Soft-fails: a missing cover
/// must not blank the course screen.
#[tauri::command]
#[specta::specta]
pub async fn cmd_lingq_course(
    app: AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    lang: String,
    collection_id: i64,
) -> Result<CourseView, AppError> {
    let client = client_for(&app, parse_lang(&lang)?)?;
    let cid = CollectionId(collection_id);
    let collection = client.collection_detail(cid).await?;
    let lessons = client.list_lesson_stats(cid).await?;
    if let Some(url) = collection.image_url.as_deref() {
        if let Err(e) =
            adopt_missing_cover(store.inner().as_ref(), client.http(), collection_id, url).await
        {
            tracing::warn!(error = %e, "lingq cover adopt skipped");
        }
    }
    Ok(CourseView {
        collection,
        lessons,
    })
}

/// Copy `image_url` onto disk for every store project that points at
/// `collection_id` and has no usable `cover_path`. Returns how many
/// projects were written. Download / type / IO failures are `Ok(0)` —
/// the caller already has the remote URL for display.
pub async fn adopt_missing_cover(
    store: &dyn ProjectStore,
    http: &reqwest::Client,
    collection_id: i64,
    image_url: &str,
) -> Result<usize, AppError> {
    let targets: Vec<_> = store
        .list()
        .map_err(|e| AppError::Other(format!("store.list: {e}")))?
        .into_iter()
        .filter(|s| {
            s.lingq_collection_id == Some(collection_id)
                && s.cover_path
                    .as_ref()
                    .map(|p| !p.exists())
                    .unwrap_or(true)
        })
        .map(|s| s.id)
        .collect();
    if targets.is_empty() {
        return Ok(0);
    }
    let Some((bytes, ext)) = download_cover_bytes(http, image_url).await? else {
        return Ok(0);
    };
    let mut n = 0;
    for id in targets {
        match set_cover_bytes_impl(store, &id, bytes.clone(), &ext) {
            Ok(_) => {
                let _ = store.update(&id, &mut |p| {
                    p.cover_uploaded_to_lingq = true;
                });
                n += 1;
            }
            Err(e) => {
                tracing::warn!(error = %e, project = %id.join_key(), "lingq cover adopt write failed");
            }
        }
    }
    Ok(n)
}

async fn download_cover_bytes(
    http: &reqwest::Client,
    url: &str,
) -> Result<Option<(Vec<u8>, String)>, AppError> {
    let resp = match http.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "lingq cover download failed");
            return Ok(None);
        }
    };
    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "lingq cover download non-success");
        return Ok(None);
    }
    let ext = ext_from(
        resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        url,
    );
    let Some(ext) = ext else {
        tracing::warn!("lingq cover: unknown image type");
        return Ok(None);
    };
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "lingq cover body read failed");
            return Ok(None);
        }
    };
    if bytes.is_empty() || bytes.len() > MAX_COVER_BYTES {
        tracing::warn!(len = bytes.len(), "lingq cover rejected by size");
        return Ok(None);
    }
    Ok(Some((bytes.to_vec(), ext)))
}

fn ext_from(content_type: Option<&str>, url: &str) -> Option<String> {
    if let Some(ct) = content_type {
        let mime = ct.split(';').next()?.trim().to_ascii_lowercase();
        match mime.as_str() {
            "image/jpeg" | "image/jpg" => return Some("jpg".into()),
            "image/png" => return Some("png".into()),
            "image/webp" => return Some("webp".into()),
            _ => {}
        }
    }
    let path = url.split('?').next().unwrap_or(url);
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .and_then(|e| match e.as_str() {
            "jpg" | "png" | "webp" => Some(e),
            "jpeg" => Some("jpg".into()),
            _ => None,
        })
}
