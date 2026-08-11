use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use futures::future::BoxFuture;
use lingq_upload_lib::commands::jobs::JobCancelMap;
use lingq_upload_lib::commands::transcribe::{
    detect_start_offset_impl, detection_availability_impl, detection_eligible,
    detection_provider_factory, load_detection_authorization,
};
use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::inspect_mismatch;
use lingq_upload_lib::core::matcher::{MismatchCondition, MismatchResponse};
use lingq_upload_lib::core::project::{MatcherDecision, Project};
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
use lingq_upload_lib::error::AppError;
use lingq_upload_lib::events::DetectionPhase;
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use lingq_upload_lib::transcribe::{
    AlignSource, DetectStartResult, DetectedRange, DetectionEvidence, DetectionSink,
    TranscribeConsent, TranscribeError, TranscribeErrorKind, TranscribeOpts, TranscribeProviderId,
    Transcriber, Transcript,
};
use secrecy::SecretString;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct AvailabilityFixture {
    _dir: TempDir,
    project: Project,
}

fn availability_project(chapter_count: usize, track_count: usize) -> AvailabilityFixture {
    let dir = tempfile::tempdir().unwrap();
    let text_paths: Vec<PathBuf> = (0..chapter_count)
        .map(|index| {
            let path = dir.path().join(format!("chapter-{index:03}.txt"));
            fs::write(&path, format!("distinct chapter body {index}")).unwrap();
            path
        })
        .collect();
    for index in 0..track_count {
        fs::write(dir.path().join(format!("track-{index:03}.wav")), b"local").unwrap();
    }
    let mut project = Project::new_test(
        ProjectId::from_title_author("Availability", "Author"),
        "Availability",
    );
    project.sources.text = TextSource::LooseFiles { paths: text_paths };
    project.sources.audio = Some(AudioSource::Folder(dir.path().to_path_buf()));
    AvailabilityFixture { _dir: dir, project }
}

fn stage_b_project() -> AvailabilityFixture {
    let mut fixture = availability_project(6, 0);
    let audio = fixture._dir.path().join("generic-audio.mp3");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/probe_3min.mp3"),
        &audio,
    )
    .unwrap();
    fixture.project.sources.audio = Some(AudioSource::Folder(fixture._dir.path().to_path_buf()));
    fixture
}

fn evidence() -> DetectionEvidence {
    DetectionEvidence {
        provider_id: Some(TranscribeProviderId::Groq),
        align_source: AlignSource::Transcript,
        range: DetectedRange {
            start_chapter_id: ChapterId::from_order(1),
            end_chapter_id: ChapterId::from_order(4),
        },
        confidence: 0.91,
        transcript_head_preview: Some("head preview".into()),
        transcript_tail_preview: Some("tail preview".into()),
        detected_at: Utc::now(),
    }
}

fn write_preferences(dir: &std::path::Path, provider_id: TranscribeProviderId) {
    fs::write(
        dir.join("transcription-preferences.json"),
        serde_json::to_vec(&serde_json::json!({
            "provider_id": provider_id,
            "auto_detect_start": true,
        }))
        .unwrap(),
    )
    .unwrap();
}

#[derive(Clone, Debug, PartialEq)]
enum DetectionEvent {
    Started,
    Progress,
    Result,
    Error(TranscribeErrorKind),
    Cancelled,
}

#[derive(Default)]
struct RecordingDetectionSink {
    events: Vec<DetectionEvent>,
}

struct CountingTranscriber {
    calls: Arc<AtomicUsize>,
}

impl Transcriber for CountingTranscriber {
    fn provider_id(&self) -> TranscribeProviderId {
        TranscribeProviderId::Groq
    }

    fn transcribe(
        &self,
        _: &Path,
        _: &TranscribeOpts,
    ) -> BoxFuture<'_, Result<Transcript, TranscribeError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(Transcript {
                text: "A deliberately unrelated provider transcript with enough useful text."
                    .into(),
            })
        })
    }
}

impl DetectionSink for RecordingDetectionSink {
    fn started(&mut self, _: Uuid) {
        self.events.push(DetectionEvent::Started);
    }

    fn progress(&mut self, _: Uuid, _: f32, _: DetectionPhase) {
        self.events.push(DetectionEvent::Progress);
    }

    fn result(&mut self, _: Uuid, _: &DetectStartResult) {
        self.events.push(DetectionEvent::Result);
    }

    fn error(&mut self, _: Uuid, error: &TranscribeError) {
        self.events.push(DetectionEvent::Error(error.kind()));
    }

    fn cancelled(&mut self, _: Uuid) {
        self.events.push(DetectionEvent::Cancelled);
    }
}

fn empty_cancel_map() -> JobCancelMap {
    Arc::new(Mutex::new(HashMap::new()))
}

#[test]
fn availability_is_chapters_heavy_and_condition_limited() {
    let cases = [
        (MismatchCondition::ManyToFew, 85, 6, true),
        (MismatchCondition::ManyToOne, 5, 1, true),
        (MismatchCondition::Unalignable, 40, 1, true),
        (MismatchCondition::OneToMany, 1, 5, false),
        (MismatchCondition::CountOff, 5, 3, false),
        (MismatchCondition::Unknown, 5, 1, false),
        (MismatchCondition::Unalignable, 0, 0, false),
        (MismatchCondition::Unalignable, 5, 0, false),
        (MismatchCondition::Unalignable, 0, 5, false),
        (MismatchCondition::Unalignable, 5, 5, false),
    ];

    for (condition, chapters, tracks, expected) in cases {
        assert_eq!(
            detection_eligible(condition, chapters, tracks),
            expected,
            "condition={condition:?}, chapters={chapters}, tracks={tracks}"
        );
    }
}

#[tokio::test]
async fn availability_reports_provider_authorization_and_start_gate() {
    let mut fixture = availability_project(6, 3);
    fixture.project.transcribe_consent = Some(TranscribeConsent {
        provider_id: TranscribeProviderId::Groq,
        accepted_at: Utc::now(),
    });

    let availability =
        detection_availability_impl(&fixture.project, TranscribeProviderId::Groq, true)
            .await
            .unwrap();

    assert!(availability.eligible);
    assert_eq!(availability.condition, Some(MismatchCondition::ManyToFew));
    assert_eq!(availability.chapter_count, 6);
    assert_eq!(availability.track_count, 3);
    assert_eq!(availability.active_provider.id, TranscribeProviderId::Groq);
    assert!(availability.active_provider.key_present);
    assert!(availability.key_present);
    assert!(availability.consent_matches);
    assert!(availability.existing_evidence.is_none());
    assert!(availability.can_start);
}

#[tokio::test]
async fn availability_reuses_confirmed_evidence_after_provider_authorization_changes() {
    let mut fixture = availability_project(6, 3);
    let expected_evidence = evidence();
    fixture.project.transcribe_consent = Some(TranscribeConsent {
        provider_id: TranscribeProviderId::Groq,
        accepted_at: Utc::now(),
    });
    fixture.project.matcher_decision = Some(MatcherDecision {
        condition: MismatchCondition::ManyToFew,
        response: MismatchResponse::SplitProportional,
        chapter_count: 6,
        track_count: 3,
        user_overrode: false,
        decided_at: Utc::now(),
        detection: Some(expected_evidence.clone()),
    });

    let availability =
        detection_availability_impl(&fixture.project, TranscribeProviderId::OpenAi, false)
            .await
            .unwrap();

    assert!(availability.eligible);
    assert_eq!(availability.condition, Some(MismatchCondition::ManyToFew));
    assert_eq!(availability.chapter_count, 6);
    assert_eq!(availability.track_count, 3);
    assert_eq!(availability.existing_evidence, Some(expected_evidence));
    assert!(!availability.key_present);
    assert!(!availability.consent_matches);
    assert!(!availability.can_start);
}

#[tokio::test]
async fn availability_inputs_do_not_change_pure_mismatch_inspection() {
    let fixture = availability_project(6, 3);
    let before = serde_json::to_value(inspect_mismatch(&fixture.project).await.unwrap()).unwrap();

    detection_availability_impl(&fixture.project, TranscribeProviderId::Groq, false)
        .await
        .unwrap();
    detection_availability_impl(&fixture.project, TranscribeProviderId::OpenAi, true)
        .await
        .unwrap();

    let after = serde_json::to_value(inspect_mismatch(&fixture.project).await.unwrap()).unwrap();
    assert_eq!(after, before);
}

#[test]
fn authorization_requires_active_provider_key_and_exact_consent_before_creation() {
    let cases = [
        (
            "missing key",
            TranscribeProviderId::Groq,
            Some(TranscribeProviderId::Groq),
            None,
            Some(TranscribeErrorKind::ApiKey),
            1,
        ),
        (
            "mismatched consent",
            TranscribeProviderId::OpenAi,
            Some(TranscribeProviderId::Groq),
            Some("openai-test-key"),
            Some(TranscribeErrorKind::Unauthorized),
            0,
        ),
        (
            "authorized",
            TranscribeProviderId::Groq,
            Some(TranscribeProviderId::Groq),
            Some("groq-test-key"),
            None,
            1,
        ),
    ];

    for (label, active_provider, consent_provider, key, expected_error, expected_key_loads) in cases
    {
        let dir = tempfile::tempdir().unwrap();
        write_preferences(dir.path(), active_provider);
        let store = InMemoryProjectStore::new();
        let mut project = Project::new_test(ProjectId::from_title_author(label, "Author"), label);
        project.transcribe_consent = consent_provider.map(|provider_id| TranscribeConsent {
            provider_id,
            accepted_at: Utc::now(),
        });
        store.put(&project).unwrap();
        let provider_creations = Arc::new(AtomicUsize::new(0));
        let key_loads = Arc::new(AtomicUsize::new(0));
        let key_loads_for_loader = Arc::clone(&key_loads);

        let authorization = load_detection_authorization(
            &store,
            &project.id,
            dir.path(),
            move |requested_provider| {
                assert_eq!(requested_provider, active_provider, "{label}");
                key_loads_for_loader.fetch_add(1, Ordering::SeqCst);
                key.map(SecretString::from).ok_or_else(|| {
                    AppError::Transcribe(TranscribeError {
                        kind: TranscribeErrorKind::ApiKey,
                        message: "no fake key".into(),
                    })
                })
            },
        );
        if authorization.is_ok() {
            provider_creations.fetch_add(1, Ordering::SeqCst);
        }

        assert_eq!(
            authorization.as_ref().err().map(|error| error.kind()),
            expected_error,
            "{label}"
        );
        assert_eq!(
            provider_creations.load(Ordering::SeqCst),
            usize::from(expected_error.is_none()),
            "{label}"
        );
        assert_eq!(
            key_loads.load(Ordering::SeqCst),
            expected_key_loads,
            "{label}"
        );
        if let Ok((provider_id, _)) = authorization {
            assert_eq!(provider_id, active_provider, "{label}");
        }
    }
}

#[tokio::test]
async fn authorization_error_starts_once_emits_one_error_and_reaps_cancel_entry() {
    let mut fixture = stage_b_project();
    fixture.project.transcribe_consent = Some(TranscribeConsent {
        provider_id: TranscribeProviderId::Groq,
        accepted_at: Utc::now(),
    });
    let preferences_dir = tempfile::tempdir().unwrap();
    write_preferences(preferences_dir.path(), TranscribeProviderId::Groq);
    let store = InMemoryProjectStore::new();
    store.put(&fixture.project).unwrap();
    let provider_creations = Arc::new(AtomicUsize::new(0));
    let provider_creations_for_factory = Arc::clone(&provider_creations);
    let cancels = empty_cancel_map();
    let mut sink = RecordingDetectionSink::default();

    let result = detect_start_offset_impl(
        &fixture.project,
        &cancels,
        Uuid::new_v4(),
        &mut sink,
        Box::new(|| {
            let _ = load_detection_authorization(
                &store,
                &fixture.project.id,
                preferences_dir.path(),
                |_| {
                    Err(AppError::Transcribe(TranscribeError {
                        kind: TranscribeErrorKind::ApiKey,
                        message: "no fake key".into(),
                    }))
                },
            )?;
            provider_creations_for_factory.fetch_add(1, Ordering::SeqCst);
            unreachable!("missing key must stop before provider creation")
        }),
    )
    .await;

    assert!(matches!(
        result,
        Err(AppError::Transcribe(ref error)) if error.kind() == TranscribeErrorKind::ApiKey
    ));
    assert_eq!(provider_creations.load(Ordering::SeqCst), 0);
    assert_eq!(sink.events.first(), Some(&DetectionEvent::Started));
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| matches!(event, DetectionEvent::Error(_)))
            .count(),
        1
    );
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| matches!(event, DetectionEvent::Result | DetectionEvent::Cancelled))
            .count(),
        0
    );
    assert!(cancels.lock().unwrap().is_empty());
}

#[tokio::test]
async fn confirmed_evidence_returns_without_starting_a_provider_job() {
    let mut fixture = stage_b_project();
    let expected_evidence = evidence();
    fixture.project.matcher_decision = Some(MatcherDecision {
        condition: MismatchCondition::ManyToFew,
        response: MismatchResponse::SplitProportional,
        chapter_count: 6,
        track_count: 1,
        user_overrode: false,
        decided_at: Utc::now(),
        detection: Some(expected_evidence.clone()),
    });
    let provider_creations = Arc::new(AtomicUsize::new(0));
    let provider_creations_for_factory = Arc::clone(&provider_creations);
    let cancels = empty_cancel_map();
    let mut sink = RecordingDetectionSink::default();

    let result = detect_start_offset_impl(
        &fixture.project,
        &cancels,
        Uuid::new_v4(),
        &mut sink,
        Box::new(move || {
            provider_creations_for_factory.fetch_add(1, Ordering::SeqCst);
            Err(TranscribeError {
                kind: TranscribeErrorKind::ProviderFailed,
                message: "provider must not be constructed".into(),
            })
        }),
    )
    .await
    .unwrap();

    let DetectStartResult::Detected { preview } = result else {
        panic!("confirmed evidence must return a detected result");
    };
    assert_eq!(preview.provider_id, expected_evidence.provider_id);
    assert_eq!(preview.align_source, expected_evidence.align_source);
    assert_eq!(preview.range, expected_evidence.range);
    assert_eq!(preview.confidence, expected_evidence.confidence);
    assert_eq!(
        preview.transcript_head_preview,
        expected_evidence.transcript_head_preview
    );
    assert_eq!(
        preview.transcript_tail_preview,
        expected_evidence.transcript_tail_preview
    );
    assert_eq!(preview.detected_at, expected_evidence.detected_at);
    assert_eq!(provider_creations.load(Ordering::SeqCst), 0);
    assert!(sink.events.is_empty());
    assert!(cancels.lock().unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_caller_job_id_is_rejected_without_replacing_the_active_token() {
    let fixture = stage_b_project();
    let job_id = Uuid::new_v4();
    let existing_token = CancellationToken::new();
    let cancels = empty_cancel_map();
    cancels
        .lock()
        .unwrap()
        .insert(job_id, (fixture.project.id.clone(), existing_token.clone()));
    let mut sink = RecordingDetectionSink::default();

    let result = detect_start_offset_impl(
        &fixture.project,
        &cancels,
        job_id,
        &mut sink,
        Box::new(|| {
            Err(TranscribeError {
                kind: TranscribeErrorKind::ProviderFailed,
                message: "duplicate job must not reach the provider".into(),
            })
        }),
    )
    .await;

    assert!(matches!(
        result,
        Err(AppError::Other(ref message)) if message.contains("already active")
    ));
    let jobs = cancels.lock().unwrap();
    assert_eq!(jobs.len(), 1);
    assert!(!existing_token.is_cancelled());
    assert!(sink.events.is_empty());
}

#[tokio::test]
async fn confirmed_evidence_reuse_still_rejects_a_duplicate_caller_job_id() {
    let mut fixture = stage_b_project();
    fixture.project.matcher_decision = Some(MatcherDecision {
        condition: MismatchCondition::ManyToFew,
        response: MismatchResponse::SplitProportional,
        chapter_count: 6,
        track_count: 1,
        user_overrode: false,
        decided_at: Utc::now(),
        detection: Some(evidence()),
    });
    let job_id = Uuid::new_v4();
    let existing_token = CancellationToken::new();
    let cancels = empty_cancel_map();
    cancels
        .lock()
        .unwrap()
        .insert(job_id, (fixture.project.id.clone(), existing_token.clone()));
    let mut sink = RecordingDetectionSink::default();

    let result = detect_start_offset_impl(
        &fixture.project,
        &cancels,
        job_id,
        &mut sink,
        Box::new(|| unreachable!("evidence reuse must not reach the provider")),
    )
    .await;

    assert!(matches!(
        result,
        Err(AppError::Other(ref message)) if message.contains("already active")
    ));
    assert_eq!(cancels.lock().unwrap().len(), 1);
    assert!(!existing_token.is_cancelled());
    assert!(sink.events.is_empty());
}

#[tokio::test]
async fn matching_authorization_constructs_one_provider_and_invokes_detection_service() {
    let mut fixture = stage_b_project();
    fixture.project.transcribe_consent = Some(TranscribeConsent {
        provider_id: TranscribeProviderId::Groq,
        accepted_at: Utc::now(),
    });
    let preferences_dir = tempfile::tempdir().unwrap();
    write_preferences(preferences_dir.path(), TranscribeProviderId::Groq);
    let store = InMemoryProjectStore::new();
    store.put(&fixture.project).unwrap();
    let provider_creations = Arc::new(AtomicUsize::new(0));
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let creations_for_factory = Arc::clone(&provider_creations);
    let calls_for_factory = Arc::clone(&provider_calls);
    let factory = detection_provider_factory(
        &store,
        &fixture.project.id,
        preferences_dir.path(),
        |_| Ok(SecretString::from("groq-test-key")),
        move |provider_id, _| {
            assert_eq!(provider_id, TranscribeProviderId::Groq);
            creations_for_factory.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(CountingTranscriber {
                calls: calls_for_factory,
            }))
        },
    );
    let cancels = empty_cancel_map();
    let mut sink = RecordingDetectionSink::default();

    let result = detect_start_offset_impl(
        &fixture.project,
        &cancels,
        Uuid::new_v4(),
        &mut sink,
        factory,
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(provider_creations.load(Ordering::SeqCst), 1);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 2);
    assert_eq!(sink.events.first(), Some(&DetectionEvent::Started));
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| matches!(event, DetectionEvent::Result))
            .count(),
        1
    );
    assert!(!sink
        .events
        .iter()
        .any(|event| matches!(event, DetectionEvent::Error(_) | DetectionEvent::Cancelled)));
    assert!(cancels.lock().unwrap().is_empty());
}
