use secrecy::SecretString;

use crate::error::AppError;
use crate::lingq::{AccountProfile, Collection, Language, LingqClient};
use crate::secrets::{RealKeyring, SecretsStore};

fn load_api_key() -> Result<String, AppError> {
    let store = SecretsStore::new(Box::new(RealKeyring::new()));
    store
        .load_key()?
        .ok_or_else(|| AppError::Internal("no LingQ API key set; configure it in Settings".into()))
}

fn client_for(lang: &str) -> Result<LingqClient, AppError> {
    let key = load_api_key()?;
    Ok(LingqClient::new(SecretString::from(key), lang))
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_account_profile() -> Result<AccountProfile, AppError> {
    // /api/v2/api-profile/ et al. are not lang-scoped; the segment is a placeholder.
    let client = client_for("en")?;
    Ok(client.account_profile().await?)
}

/// Returns the caller's enrolled languages. With a username we get the
/// user-trimmed catalogue (matches the browser extension); without one we
/// fall back to the full catalogue and let the UI filter by known_words.
#[tauri::command]
#[specta::specta]
pub async fn cmd_list_languages(username: Option<String>) -> Result<Vec<Language>, AppError> {
    let client = client_for("en")?;
    let langs = match username.as_deref() {
        Some(u) if !u.is_empty() => client.list_my_languages_for(u).await?,
        _ => client.list_my_languages().await?,
    };
    Ok(langs)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_list_collections(lang: String) -> Result<Vec<Collection>, AppError> {
    let client = client_for(&lang)?;
    Ok(client.list_my_collections().await?)
}
