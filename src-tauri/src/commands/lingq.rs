use secrecy::SecretString;
use tauri::AppHandle;

use super::{app_data_dir, parse_lang};
use crate::error::AppError;
use crate::lingq::{AccountProfile, Collection, Language, LanguageCode, LingqClient};
use crate::secrets::SecretsStore;

fn load_api_key(app: &AppHandle) -> Result<String, AppError> {
    let store = SecretsStore::new_default(&app_data_dir(app)?);
    store
        .load_key()?
        .ok_or_else(|| AppError::Internal("no LingQ API key set; configure it in Settings".into()))
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
