use secrecy::SecretString;

use crate::error::AppError;
use crate::lingq::{Collection, Language, LingqClient};
use crate::secrets::{RealKeyring, SecretsStore};

fn load_api_key() -> Result<String, AppError> {
    let store = SecretsStore::new(Box::new(RealKeyring::new()));
    store.load_key()?.ok_or_else(|| {
        AppError::Internal("no LingQ API key set; configure it in Settings".into())
    })
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_list_languages() -> Result<Vec<Language>, AppError> {
    let key = load_api_key()?;
    // /api/v2/languages/ is not lang-scoped; the segment is a placeholder.
    let client = LingqClient::new(SecretString::from(key), "en");
    Ok(client.list_my_languages().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_list_collections(lang: String) -> Result<Vec<Collection>, AppError> {
    let key = load_api_key()?;
    let client = LingqClient::new(SecretString::from(key), &lang);
    Ok(client.list_my_collections().await?)
}
