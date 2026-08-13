use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use uuid::Uuid;

use super::jobs::{register_caller_job, JobCancelMap};
use super::{app_data_dir, secrets};
use crate::core::identity::ProjectId;
use crate::core::job::{
    detection_chapters, inspect_mismatch, resolve_audio_tracks, seed_bounded_mapping,
    seed_mapping_for_response,
};
use crate::core::matcher::{MismatchCondition, MismatchResponse};
use crate::core::project::{MatcherDecision, Project};
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::events::JobEmitter;
use crate::secrets::{KeyringBackend, SecretsStore, GROQ_ACCOUNT, OPENAI_ACCOUNT};
use crate::transcribe::{
    bound_preview, consent_matches, detect_start_offset, detection_provider_matches_source,
    AlignmentConfig, DetectStartResult, DetectedRange, DetectionEvidence, DetectionPreview,
    DetectionSink, ProviderCatalog, ProviderDescriptor, ProviderFactory, TranscribeConsent,
    TranscribeError, TranscribeErrorKind, TranscribeProviderId, Transcriber,
};

const PREFERENCES_FILE: &str = "transcription-preferences.json";
const PROVIDER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

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

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct DetectionAvailability {
    pub eligible: bool,
    pub condition: Option<MismatchCondition>,
    pub chapter_count: usize,
    pub track_count: usize,
    pub active_provider: ProviderInfo,
    pub key_present: bool,
    pub consent_matches: bool,
    pub existing_evidence: Option<DetectionEvidence>,
    pub can_start: bool,
}

pub fn detection_eligible(
    condition: MismatchCondition,
    chapter_count: usize,
    track_count: usize,
) -> bool {
    chapter_count > track_count
        && track_count > 0
        && matches!(
            condition,
            MismatchCondition::ManyToFew
                | MismatchCondition::ManyToOne
                | MismatchCondition::Unalignable
        )
}

pub async fn detection_availability_impl(
    project: &Project,
    active_provider: TranscribeProviderId,
    key_present: bool,
) -> Result<DetectionAvailability, AppError> {
    let inspection = inspect_mismatch(project).await?;
    let (condition, chapter_count, track_count) = inspection
        .map(|inspection| {
            (
                Some(inspection.condition),
                inspection.chapter_count,
                inspection.track_count,
            )
        })
        .or_else(|| {
            project.matcher_decision.as_ref().map(|decision| {
                (
                    Some(decision.condition),
                    decision.chapter_count,
                    decision.track_count,
                )
            })
        })
        .unwrap_or((None, 0, 0));
    let eligible = condition
        .is_some_and(|condition| detection_eligible(condition, chapter_count, track_count));
    let consent_matches = consent_matches(project.transcribe_consent.as_ref(), active_provider);
    let existing_evidence = project
        .matcher_decision
        .as_ref()
        .and_then(|decision| decision.detection.clone());
    let can_start = eligible && key_present && consent_matches && existing_evidence.is_none();
    let descriptor = ProviderCatalog::built_in().descriptor(active_provider)?;

    Ok(DetectionAvailability {
        eligible,
        condition,
        chapter_count,
        track_count,
        active_provider: provider_info(descriptor, key_present),
        key_present,
        consent_matches,
        existing_evidence,
        can_start,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_detection_availability(
    app: AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
) -> Result<DetectionAvailability, AppError> {
    let project = store
        .get(&project_id)
        .map_err(|error| AppError::Other(format!("store.get: {error}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    let preferences = load_preferences(&app_data_dir(&app)?)?;
    let key_present = key_present(preferences.provider_id, secrets::backend(&app)?)?;
    detection_availability_impl(&project, preferences.provider_id, key_present).await
}

fn detection_operational_error(error: AppError) -> TranscribeError {
    match error {
        AppError::Transcribe(error) => error,
        error => TranscribeError::new(TranscribeErrorKind::ProviderFailed, error.to_string()),
    }
}

pub fn load_detection_authorization(
    store: &dyn ProjectStore,
    project_id: &ProjectId,
    app_data_dir: &Path,
    load_active_key: impl FnOnce(TranscribeProviderId) -> Result<SecretString, AppError>,
) -> Result<(TranscribeProviderId, SecretString), TranscribeError> {
    let preferences = load_preferences(app_data_dir).map_err(detection_operational_error)?;
    let project = store
        .get(project_id)
        .map_err(|error| {
            detection_operational_error(AppError::Other(format!("store.get: {error}")))
        })?
        .ok_or_else(|| {
            TranscribeError::new(TranscribeErrorKind::ProviderFailed, "project not found")
        })?;
    if !consent_matches(project.transcribe_consent.as_ref(), preferences.provider_id) {
        return Err(TranscribeError::new(
            TranscribeErrorKind::Unauthorized,
            format!(
                "transcription consent does not match active provider {:?}",
                preferences.provider_id
            ),
        ));
    }
    let key = load_active_key(preferences.provider_id).map_err(detection_operational_error)?;
    Ok((preferences.provider_id, key))
}

pub fn detection_provider_factory<'a>(
    store: &'a dyn ProjectStore,
    project_id: &'a ProjectId,
    app_data_dir: &'a Path,
    load_active_key: impl FnOnce(TranscribeProviderId) -> Result<SecretString, AppError> + Send + 'a,
    create_provider: impl FnOnce(TranscribeProviderId, SecretString) -> Result<Box<dyn Transcriber>, TranscribeError>
        + Send
        + 'a,
) -> ProviderFactory<'a> {
    Box::new(move || {
        let (provider_id, key) =
            load_detection_authorization(store, project_id, app_data_dir, load_active_key)?;
        create_provider(provider_id, key)
    })
}

pub async fn detect_start_offset_impl(
    project: &Project,
    cancels: &JobCancelMap,
    job_id: Uuid,
    sink: &mut dyn DetectionSink,
    provider_factory: ProviderFactory<'_>,
) -> Result<DetectStartResult, AppError> {
    let (_guard, cancel) = register_caller_job(cancels, job_id, &project.id)?;
    if let Some(evidence) = project
        .matcher_decision
        .as_ref()
        .and_then(|decision| decision.detection.as_ref())
    {
        return Ok(DetectStartResult::Detected {
            preview: DetectionPreview {
                provider_id: evidence.provider_id,
                align_source: evidence.align_source,
                range: evidence.range.clone(),
                confidence: evidence.confidence,
                transcript_head_preview: evidence.transcript_head_preview.clone(),
                transcript_tail_preview: evidence.transcript_tail_preview.clone(),
                detected_at: evidence.detected_at,
            },
        });
    }
    let tracks = resolve_audio_tracks(project).await?;
    let chapters = detection_chapters(project)?;
    detect_start_offset(
        project,
        &tracks,
        &chapters,
        job_id,
        &AlignmentConfig::default(),
        sink,
        cancel,
        provider_factory,
    )
    .await
    .map_err(AppError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_detect_start_offset(
    app: AppHandle,
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    cancels: tauri::State<'_, JobCancelMap>,
    project_id: ProjectId,
    job_id: Uuid,
) -> Result<DetectStartResult, AppError> {
    let project = store
        .get(&project_id)
        .map_err(|error| AppError::Other(format!("store.get: {error}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    let data_dir = app_data_dir(&app)?;
    let app_for_key = app.clone();
    let factory = detection_provider_factory(
        store.inner().as_ref(),
        &project_id,
        &data_dir,
        move |provider_id| load_key(provider_id, secrets::backend(&app_for_key)?),
        |provider_id, key| {
            let http_client = reqwest::Client::builder()
                .connect_timeout(PROVIDER_CONNECT_TIMEOUT)
                .timeout(PROVIDER_REQUEST_TIMEOUT)
                .build()
                .map_err(|error| {
                    TranscribeError::new(TranscribeErrorKind::Network, error.to_string())
                })?;
            ProviderCatalog::built_in().create(provider_id, key, http_client)
        },
    );
    let mut emitter = JobEmitter::new(&app, job_id);
    detect_start_offset_impl(&project, cancels.inner(), job_id, &mut emitter, factory).await
}

fn validate_detection_preview(preview: &DetectionPreview) -> Result<(), AppError> {
    if !preview.confidence.is_finite() || !(0.0..=1.0).contains(&preview.confidence) {
        return Err(AppError::Unsupported(
            "detection confidence must be finite and within 0.0..=1.0".into(),
        ));
    }
    if !detection_provider_matches_source(preview.align_source, preview.provider_id) {
        return Err(AppError::Unsupported(
            "detection provider does not match the alignment source".into(),
        ));
    }
    let bounded = |value: Option<&str>| bound_preview(value).as_deref() == value;
    if !bounded(preview.transcript_head_preview.as_deref())
        || !bounded(preview.transcript_tail_preview.as_deref())
    {
        return Err(AppError::Unsupported(
            "detection transcript previews must not exceed 240 Unicode scalars".into(),
        ));
    }
    Ok(())
}

fn detection_inputs_unchanged(resolved: &Project, current: &Project) -> bool {
    current.sources == resolved.sources
        && current.cover_source_href == resolved.cover_source_href
        && current.skipped_chapters == resolved.skipped_chapters
        && current.receipts == resolved.receipts
        && current.matcher_decision == resolved.matcher_decision
        && current.mapping == resolved.mapping
        && current.confirmed_at == resolved.confirmed_at
}

fn stale_text_source(error: AppError) -> AppError {
    match error {
        AppError::Text(_) => {
            AppError::Unsupported("text source changed; rerun or refine detection".into())
        }
        error => error,
    }
}

pub async fn confirm_detected_range_impl(
    store: &dyn ProjectStore,
    project_id: &ProjectId,
    selected_range: DetectedRange,
    preview: DetectionPreview,
) -> Result<(), AppError> {
    validate_detection_preview(&preview)?;
    let project = store
        .get(project_id)
        .map_err(|error| AppError::Other(format!("store.get: {error}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    let inspection = inspect_mismatch(&project)
        .await
        .map_err(stale_text_source)?
        .filter(|inspection| {
            detection_eligible(
                inspection.condition,
                inspection.chapter_count,
                inspection.track_count,
            )
        })
        .ok_or_else(|| {
            AppError::Unsupported("project is not eligible for text-range detection".into())
        })?;
    if preview.range != selected_range {
        seed_bounded_mapping(&project, &preview.range)
            .await
            .map_err(stale_text_source)?;
    }
    let mapping = if selected_range.start_chapter_id == selected_range.end_chapter_id
        && inspection.chapter_count > 1
    {
        seed_mapping_for_response(&project, MismatchResponse::SplitProportional)
            .await
            .map_err(stale_text_source)?
            .ok_or_else(|| AppError::Other("split-proportional needs chapters and tracks".into()))?
    } else {
        seed_bounded_mapping(&project, &selected_range)
            .await
            .map_err(stale_text_source)?
    };
    let now = Utc::now();
    let evidence = DetectionEvidence {
        provider_id: preview.provider_id,
        align_source: preview.align_source,
        range: selected_range,
        confidence: preview.confidence,
        transcript_head_preview: preview.transcript_head_preview,
        transcript_tail_preview: preview.transcript_tail_preview,
        detected_at: now,
    };
    let decision = MatcherDecision {
        condition: inspection.condition,
        response: MismatchResponse::SplitProportional,
        chapter_count: inspection.chapter_count,
        track_count: inspection.track_count,
        user_overrode: inspection.preselect != MismatchResponse::SplitProportional,
        decided_at: now,
        detection: Some(evidence),
    };
    let mut confirmed = false;
    store
        .update(project_id, &mut |current| {
            if detection_inputs_unchanged(&project, current) {
                current.matcher_decision = Some(decision.clone());
                current.mapping = Some(mapping.clone());
                current.confirmed_at = None;
                confirmed = true;
            }
        })
        .map_err(|error| AppError::Other(format!("store.update: {error}")))?;
    if !confirmed {
        return Err(AppError::Unsupported(
            "project changed while confirming detection; reload and try again".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_confirm_detected_range(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    selected_range: DetectedRange,
    evidence: DetectionPreview,
) -> Result<(), AppError> {
    confirm_detected_range_impl(
        store.inner().as_ref(),
        &project_id,
        selected_range,
        evidence,
    )
    .await
}

pub fn reset_detection_impl(
    store: &dyn ProjectStore,
    project_id: &ProjectId,
) -> Result<(), AppError> {
    let project = store
        .get(project_id)
        .map_err(|error| AppError::Other(format!("store.get: {error}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    if !project.receipts.is_empty() {
        return Err(AppError::Unsupported(
            "cannot reset detection after uploads have begun".into(),
        ));
    }
    if project
        .matcher_decision
        .as_ref()
        .and_then(|decision| decision.detection.as_ref())
        .is_none()
    {
        return Err(AppError::Unsupported(
            "project has no confirmed detection to reset".into(),
        ));
    }

    let mut reset = false;
    let mut uploads_started = false;
    store
        .update(project_id, &mut |project| {
            if !project.receipts.is_empty() {
                uploads_started = true;
                return;
            }
            if project
                .matcher_decision
                .as_ref()
                .and_then(|decision| decision.detection.as_ref())
                .is_some()
            {
                project.matcher_decision = None;
                project.mapping = None;
                project.confirmed_at = None;
                reset = true;
            }
        })
        .map_err(|error| AppError::Other(format!("store.update: {error}")))?;
    if uploads_started {
        return Err(AppError::Unsupported(
            "cannot reset detection after uploads have begun".into(),
        ));
    }
    if !reset {
        return Err(AppError::Unsupported(
            "confirmed detection changed; reload the project and try again".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_reset_detection(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
) -> Result<(), AppError> {
    reset_detection_impl(store.inner().as_ref(), &project_id)
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
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::core::identity::ProjectId;
    use crate::core::project::Project;
    use crate::core::store::{InMemoryProjectStore, ProjectStore};
    use crate::secrets::InMemoryKeyring;

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
        let backend = InMemoryKeyring::default();
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
        let error = load_key(
            TranscribeProviderId::Groq,
            Box::new(InMemoryKeyring::default()),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AppError::Transcribe(error) if error.kind() == TranscribeErrorKind::ApiKey
        ));
    }

    #[test]
    fn provider_list_reports_presence_without_exposing_keys() {
        let backend = InMemoryKeyring::default();
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
        let backend = InMemoryKeyring::default();
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
