use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use super::{app_data_dir, secrets};
use crate::core::identity::ProjectId;
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::secrets::{KeyringBackend, SecretsStore, GROQ_ACCOUNT, OPENAI_ACCOUNT};
use crate::transcribe::{
    ProviderCatalog, ProviderDescriptor, TranscribeConsent, TranscribeError, TranscribeErrorKind,
    TranscribeProviderId,
};

const PREFERENCES_FILE: &str = "transcription-preferences.json";

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct AppTranscriptionPreferences {
    pub provider_id: TranscribeProviderId,
    pub auto_detect_start: bool,
}

impl Default for AppTranscriptionPreferences {
    fn default() -> Self {
        Self {
            provider_id: TranscribeProviderId::Groq,
            auto_detect_start: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct ProviderInfo {
    pub id: TranscribeProviderId,
    pub label: String,
    pub model: String,
    pub pricing_hint: PricingHintDto,
    pub data_policy_url: String,
    pub key_present: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct PricingHintDto {
    pub summary: String,
    pub estimated_usd_per_minute: Option<f64>,
    pub free_tier_eligible: bool,
    pub docs_url: String,
}

fn validate_provider(provider: TranscribeProviderId) -> Result<(), AppError> {
    ProviderCatalog::built_in()
        .descriptor(provider)
        .map(|_| ())
        .map_err(|error| AppError::Unsupported(error.to_string()))
}

fn provider_account(provider: TranscribeProviderId) -> Result<&'static str, AppError> {
    validate_provider(provider)?;
    Ok(match provider {
        TranscribeProviderId::Groq => GROQ_ACCOUNT,
        TranscribeProviderId::OpenAi => OPENAI_ACCOUNT,
    })
}

fn provider_store(
    provider: TranscribeProviderId,
    backend: Box<dyn KeyringBackend>,
) -> Result<SecretsStore, AppError> {
    Ok(SecretsStore::new(provider_account(provider)?, backend))
}

fn save_key(
    provider: TranscribeProviderId,
    key: &str,
    backend: Box<dyn KeyringBackend>,
) -> Result<(), AppError> {
    provider_store(provider, backend)?.save_key(key)?;
    Ok(())
}

fn key_present(
    provider: TranscribeProviderId,
    backend: Box<dyn KeyringBackend>,
) -> Result<bool, AppError> {
    match load_key(provider, backend) {
        Ok(_) => Ok(true),
        Err(AppError::Transcribe(error)) if error.kind() == TranscribeErrorKind::ApiKey => {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn load_key(
    provider: TranscribeProviderId,
    backend: Box<dyn KeyringBackend>,
) -> Result<SecretString, AppError> {
    let key = provider_store(provider, backend)?
        .load_key()?
        .ok_or_else(|| {
            TranscribeError::new(
                TranscribeErrorKind::ApiKey,
                format!("no transcription API key configured for {provider:?}"),
            )
        })?;
    Ok(SecretString::from(key))
}

fn clear_key(
    provider: TranscribeProviderId,
    backend: Box<dyn KeyringBackend>,
) -> Result<(), AppError> {
    provider_store(provider, backend)?.clear_key()?;
    Ok(())
}

fn provider_info(descriptor: &ProviderDescriptor, key_present: bool) -> ProviderInfo {
    ProviderInfo {
        id: descriptor.id,
        label: descriptor.label.to_owned(),
        model: descriptor.model.to_owned(),
        pricing_hint: PricingHintDto {
            summary: descriptor.pricing.summary.to_owned(),
            estimated_usd_per_minute: descriptor.pricing.estimated_usd_per_minute,
            free_tier_eligible: descriptor.pricing.free_tier_eligible,
            docs_url: descriptor.pricing.docs_url.to_owned(),
        },
        data_policy_url: descriptor.data_policy_url.to_owned(),
        key_present,
    }
}

fn list_providers(
    mut key_present: impl FnMut(TranscribeProviderId) -> Result<bool, AppError>,
) -> Result<Vec<ProviderInfo>, AppError> {
    ProviderCatalog::built_in()
        .descriptors()
        .iter()
        .map(|descriptor| Ok(provider_info(descriptor, key_present(descriptor.id)?)))
        .collect()
}

fn preferences_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join(PREFERENCES_FILE)
}

fn load_preferences(app_data_dir: &Path) -> Result<AppTranscriptionPreferences, AppError> {
    let path = preferences_path(app_data_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppTranscriptionPreferences::default());
        }
        Err(error) => {
            return Err(AppError::Io(format!(
                "transcription preferences read {}: {error}",
                path.display()
            )));
        }
    };
    let preferences: AppTranscriptionPreferences =
        serde_json::from_slice(&bytes).map_err(|error| {
            AppError::Other(format!(
                "transcription preferences parse {}: {error}",
                path.display()
            ))
        })?;
    validate_provider(preferences.provider_id)?;
    Ok(preferences)
}

fn save_preferences(
    app_data_dir: &Path,
    preferences: &AppTranscriptionPreferences,
) -> Result<(), AppError> {
    validate_provider(preferences.provider_id)?;
    std::fs::create_dir_all(app_data_dir).map_err(|error| {
        AppError::Io(format!(
            "transcription preferences create directory {}: {error}",
            app_data_dir.display()
        ))
    })?;
    let path = preferences_path(app_data_dir);
    let bytes = serde_json::to_vec_pretty(preferences).map_err(|error| {
        AppError::Internal(format!(
            "transcription preferences encode {}: {error}",
            path.display()
        ))
    })?;
    let mut temporary = tempfile::NamedTempFile::new_in(app_data_dir).map_err(|error| {
        AppError::Io(format!(
            "transcription preferences create temporary file in {}: {error}",
            app_data_dir.display()
        ))
    })?;
    temporary.write_all(&bytes).map_err(|error| {
        AppError::Io(format!(
            "transcription preferences write {}: {error}",
            path.display()
        ))
    })?;
    temporary.flush().map_err(|error| {
        AppError::Io(format!(
            "transcription preferences flush {}: {error}",
            path.display()
        ))
    })?;
    temporary.as_file().sync_all().map_err(|error| {
        AppError::Io(format!(
            "transcription preferences sync {}: {error}",
            path.display()
        ))
    })?;
    temporary.persist(&path).map_err(|error| {
        AppError::Io(format!(
            "transcription preferences persist {}: {}",
            path.display(),
            error.error
        ))
    })?;

    #[cfg(unix)]
    std::fs::File::open(app_data_dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            AppError::Io(format!(
                "transcription preferences sync directory {}: {error}",
                app_data_dir.display()
            ))
        })?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn cmd_save_transcribe_key(
    app: AppHandle,
    provider: TranscribeProviderId,
    key: String,
) -> Result<(), AppError> {
    save_key(provider, &key, secrets::backend(&app)?)
}

#[tauri::command]
#[specta::specta]
pub fn cmd_transcribe_key_present(
    app: AppHandle,
    provider: TranscribeProviderId,
) -> Result<bool, AppError> {
    key_present(provider, secrets::backend(&app)?)
}

#[tauri::command]
#[specta::specta]
pub fn cmd_clear_transcribe_key(
    app: AppHandle,
    provider: TranscribeProviderId,
) -> Result<(), AppError> {
    clear_key(provider, secrets::backend(&app)?)
}

#[tauri::command]
#[specta::specta]
pub fn cmd_list_transcribe_providers(app: AppHandle) -> Result<Vec<ProviderInfo>, AppError> {
    list_providers(|provider| key_present(provider, secrets::backend(&app)?))
}

#[tauri::command]
#[specta::specta]
pub fn cmd_get_transcription_preferences(
    app: AppHandle,
) -> Result<AppTranscriptionPreferences, AppError> {
    load_preferences(&app_data_dir(&app)?)
}

#[tauri::command]
#[specta::specta]
pub fn cmd_set_transcription_preferences(
    app: AppHandle,
    preferences: AppTranscriptionPreferences,
) -> Result<(), AppError> {
    save_preferences(&app_data_dir(&app)?, &preferences)
}

fn accept_transcribe_consent(
    store: &dyn ProjectStore,
    project_id: &ProjectId,
    provider_id: TranscribeProviderId,
) -> Result<(), AppError> {
    validate_provider(provider_id)?;
    let consent = TranscribeConsent {
        provider_id,
        accepted_at: Utc::now(),
    };
    store
        .update(project_id, &mut |project| {
            project.transcribe_consent = Some(consent.clone());
        })
        .map_err(|error| AppError::Other(format!("store.update: {error}")))?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_accept_transcribe_consent(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    provider_id: TranscribeProviderId,
) -> Result<(), AppError> {
    accept_transcribe_consent(store.inner().as_ref(), &project_id, provider_id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::sync::{Arc, Mutex};

    use tempfile::tempdir;

    use super::*;
    use crate::core::identity::ProjectId;
    use crate::core::project::Project;
    use crate::core::store::{InMemoryProjectStore, ProjectStore};
    use crate::secrets::{KeyringBackend, SecretError};

    #[derive(Clone, Default)]
    struct FakeBackend {
        entries: Arc<Mutex<HashMap<(String, String), String>>>,
    }

    impl KeyringBackend for FakeBackend {
        fn set(&self, service: &str, account: &str, value: &str) -> Result<(), SecretError> {
            self.entries
                .lock()
                .expect("fake entries lock")
                .insert((service.into(), account.into()), value.into());
            Ok(())
        }

        fn get(&self, service: &str, account: &str) -> Result<Option<String>, SecretError> {
            Ok(self
                .entries
                .lock()
                .expect("fake entries lock")
                .get(&(service.into(), account.into()))
                .cloned())
        }

        fn delete(&self, service: &str, account: &str) -> Result<(), SecretError> {
            self.entries
                .lock()
                .expect("fake entries lock")
                .remove(&(service.into(), account.into()));
            Ok(())
        }
    }

    fn prefs(
        provider_id: TranscribeProviderId,
        auto_detect_start: bool,
    ) -> AppTranscriptionPreferences {
        AppTranscriptionPreferences {
            provider_id,
            auto_detect_start,
        }
    }

    #[test]
    fn consent_for_a_registered_provider_is_persisted_with_server_time() {
        let store = InMemoryProjectStore::new();
        let id = ProjectId::from_title_author("Consent", "Author");
        let project = Project::new_test(id.clone(), "Consent");
        store.put(&project).unwrap();
        let before = chrono::Utc::now();

        accept_transcribe_consent(&store, &id, TranscribeProviderId::OpenAi).unwrap();

        let after = chrono::Utc::now();
        let saved = store.get(&id).unwrap().unwrap();
        let consent = saved.transcribe_consent.clone().unwrap();
        assert_eq!(consent.provider_id, TranscribeProviderId::OpenAi);
        assert!(consent.accepted_at >= before);
        assert!(consent.accepted_at <= after);
        let mut expected = project;
        expected.transcribe_consent = Some(consent);
        assert_eq!(saved, expected);
    }

    #[test]
    fn consent_rejects_an_unknown_serialized_provider_before_mutation() {
        let store = InMemoryProjectStore::new();
        let id = ProjectId::from_title_author("Consent", "Author");
        store
            .put(&Project::new_test(id.clone(), "Consent"))
            .unwrap();

        let provider = serde_json::from_str::<TranscribeProviderId>(r#""not_registered""#);
        if let Ok(provider) = provider {
            accept_transcribe_consent(&store, &id, provider).unwrap();
        }

        assert!(provider.is_err());
        assert!(store
            .get(&id)
            .unwrap()
            .unwrap()
            .transcribe_consent
            .is_none());
    }

    #[test]
    fn preferences_default_and_atomic_round_trip() {
        let dir = tempdir().unwrap();
        assert_eq!(
            load_preferences(dir.path()).unwrap(),
            AppTranscriptionPreferences::default()
        );

        let expected = prefs(TranscribeProviderId::OpenAi, true);
        save_preferences(dir.path(), &expected).unwrap();

        assert_eq!(load_preferences(dir.path()).unwrap(), expected);
        assert!(dir.path().join("transcription-preferences.json").is_file());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn malformed_preferences_are_actionable_and_not_replaced_with_defaults() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("transcription-preferences.json");
        fs::write(&path, b"{ definitely not json").unwrap();

        let error = load_preferences(dir.path()).unwrap_err().to_string();

        assert!(error.contains("transcription-preferences.json"));
        assert!(error.contains("parse"));
        assert_eq!(fs::read(&path).unwrap(), b"{ definitely not json");
    }

    #[test]
    fn unknown_provider_preferences_are_rejected_before_state_change() {
        let dir = tempdir().unwrap();
        let existing = prefs(TranscribeProviderId::Groq, false);
        save_preferences(dir.path(), &existing).unwrap();

        let incoming = serde_json::from_str::<AppTranscriptionPreferences>(
            r#"{"provider_id":"not_registered","auto_detect_start":true}"#,
        );

        assert!(incoming.is_err());
        assert_eq!(load_preferences(dir.path()).unwrap(), existing);
    }

    #[test]
    fn provider_keys_have_independent_presence_and_clear() {
        let backend = FakeBackend::default();
        save_key(
            TranscribeProviderId::Groq,
            "groq-key",
            Box::new(backend.clone()),
        )
        .unwrap();
        save_key(
            TranscribeProviderId::OpenAi,
            "openai-key",
            Box::new(backend.clone()),
        )
        .unwrap();

        clear_key(TranscribeProviderId::Groq, Box::new(backend.clone())).unwrap();

        assert!(!key_present(TranscribeProviderId::Groq, Box::new(backend.clone())).unwrap());
        assert!(key_present(TranscribeProviderId::OpenAi, Box::new(backend)).unwrap());
    }

    #[test]
    fn missing_provider_key_maps_to_typed_api_key_error() {
        let error =
            load_key(TranscribeProviderId::Groq, Box::new(FakeBackend::default())).unwrap_err();

        assert!(matches!(
            error,
            AppError::Transcribe(error) if error.kind() == TranscribeErrorKind::ApiKey
        ));
    }

    #[test]
    fn provider_list_reports_presence_without_exposing_keys() {
        let backend = FakeBackend::default();
        save_key(
            TranscribeProviderId::Groq,
            "groq-key",
            Box::new(backend.clone()),
        )
        .unwrap();

        let providers =
            list_providers(|provider| key_present(provider, Box::new(backend.clone()))).unwrap();

        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].id, TranscribeProviderId::Groq);
        assert_eq!(providers[0].label, "Groq");
        assert_eq!(providers[0].model, "whisper-large-v3-turbo");
        assert!(providers[0].key_present);
        assert_eq!(providers[1].id, TranscribeProviderId::OpenAi);
        assert_eq!(providers[1].label, "OpenAI");
        assert_eq!(providers[1].model, "whisper-1");
        assert!(!providers[1].key_present);
    }

    #[test]
    fn switching_provider_preserves_keys_and_project_records() {
        let dir = tempdir().unwrap();
        let project_path = dir.path().join("projects/book/project.json");
        fs::create_dir_all(project_path.parent().unwrap()).unwrap();
        fs::write(&project_path, b"project sentinel").unwrap();
        let backend = FakeBackend::default();
        save_key(
            TranscribeProviderId::Groq,
            "groq-key",
            Box::new(backend.clone()),
        )
        .unwrap();
        save_key(
            TranscribeProviderId::OpenAi,
            "openai-key",
            Box::new(backend.clone()),
        )
        .unwrap();

        save_preferences(dir.path(), &prefs(TranscribeProviderId::Groq, false)).unwrap();
        save_preferences(dir.path(), &prefs(TranscribeProviderId::OpenAi, true)).unwrap();

        assert!(key_present(TranscribeProviderId::Groq, Box::new(backend.clone())).unwrap());
        assert!(key_present(TranscribeProviderId::OpenAi, Box::new(backend)).unwrap());
        assert_eq!(fs::read(project_path).unwrap(), b"project sentinel");
    }
}
