use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{Duration as ChronoDuration, Utc};
use futures::future::BoxFuture;
use lingq_upload_lib::commands::jobs::JobCancelMap;
use lingq_upload_lib::commands::project::confirm_mapping_impl;
use lingq_upload_lib::commands::transcribe::{
    confirm_detected_range_impl, detect_start_offset_impl, detection_availability_impl,
    detection_eligible, detection_provider_factory, load_detection_authorization,
    reset_detection_impl,
};
use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::{inspect_mismatch, plan_preview};
use lingq_upload_lib::core::matcher::{
    allowed, MappingOp, MappingState, MismatchCondition, MismatchResponse,
};
use lingq_upload_lib::core::project::{ChapterReceipt, MatcherDecision, Project, ProjectSummary};
use lingq_upload_lib::core::store::{
    InMemoryProjectStore, JsonProjectStore, ProjectStore, StoreError,
};
use lingq_upload_lib::error::AppError;
use lingq_upload_lib::events::DetectionPhase;
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use lingq_upload_lib::transcribe::{
    AlignSource, AtomStart, DetectStartResult, DetectedRange, DetectionEvidence, DetectionPreview,
    DetectionSink, TranscribeConsent, TranscribeError, TranscribeErrorKind, TranscribeOpts,
    TranscribeProviderId, Transcriber, Transcript,
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

fn consented(
    mut fixture: AvailabilityFixture,
    provider_id: TranscribeProviderId,
) -> AvailabilityFixture {
    fixture.project.transcribe_consent = Some(TranscribeConsent {
        provider_id,
        accepted_at: Utc::now(),
    });
    fixture
}

/// A project whose consent already matches the Groq default of the matrix.
fn groq_project(chapter_count: usize, track_count: usize) -> AvailabilityFixture {
    consented(
        availability_project(chapter_count, track_count),
        TranscribeProviderId::Groq,
    )
}

fn with_evidence(
    mut fixture: AvailabilityFixture,
    chapter_count: usize,
    track_count: usize,
) -> AvailabilityFixture {
    fixture.project.matcher_decision = Some(MatcherDecision {
        condition: MismatchCondition::ManyToFew,
        response: MismatchResponse::SplitProportional,
        chapter_count,
        track_count,
        user_overrode: false,
        decided_at: Utc::now(),
        detection: Some(evidence()),
    });
    fixture
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
        atom_starts: Vec::new(),
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

struct UpdateProbeStore<'a> {
    inner: &'a dyn ProjectStore,
    update_calls: AtomicUsize,
    behavior: UpdateBehavior,
}

#[derive(Clone, Copy)]
enum UpdateBehavior {
    Delegate,
    Fail,
    ChangeSelection,
    AddReceipt,
}

impl<'a> UpdateProbeStore<'a> {
    fn new(inner: &'a dyn ProjectStore) -> Self {
        Self {
            inner,
            update_calls: AtomicUsize::new(0),
            behavior: UpdateBehavior::Delegate,
        }
    }

    fn failing(inner: &'a dyn ProjectStore) -> Self {
        Self {
            inner,
            update_calls: AtomicUsize::new(0),
            behavior: UpdateBehavior::Fail,
        }
    }

    fn changing_selection(inner: &'a dyn ProjectStore) -> Self {
        Self {
            inner,
            update_calls: AtomicUsize::new(0),
            behavior: UpdateBehavior::ChangeSelection,
        }
    }

    fn adding_receipt(inner: &'a dyn ProjectStore) -> Self {
        Self {
            inner,
            update_calls: AtomicUsize::new(0),
            behavior: UpdateBehavior::AddReceipt,
        }
    }

    fn update_calls(&self) -> usize {
        self.update_calls.load(Ordering::SeqCst)
    }
}

impl ProjectStore for UpdateProbeStore<'_> {
    fn put(&self, project: &Project) -> Result<(), StoreError> {
        self.inner.put(project)
    }

    fn get(&self, id: &ProjectId) -> Result<Option<Project>, StoreError> {
        self.inner.get(id)
    }

    fn project_dir(&self, id: &ProjectId) -> Option<PathBuf> {
        self.inner.project_dir(id)
    }

    fn update(
        &self,
        id: &ProjectId,
        f: &mut dyn FnMut(&mut Project),
    ) -> Result<Project, StoreError> {
        self.update_calls.fetch_add(1, Ordering::SeqCst);
        match self.behavior {
            UpdateBehavior::Fail => {
                let mut project = self
                    .inner
                    .get(id)?
                    .ok_or_else(|| StoreError::NotFound { key: id.join_key() })?;
                f(&mut project);
                return Err(StoreError::Io {
                    path: PathBuf::from("injected-update-failure"),
                    message: "injected update failure after closure".into(),
                });
            }
            UpdateBehavior::ChangeSelection => {
                self.inner.update(id, &mut |project| {
                    project.skipped_chapters.push(ChapterId::from_order(0));
                })?;
            }
            UpdateBehavior::AddReceipt => {
                self.inner.update(id, &mut |project| {
                    project.receipts.push(interleaved_receipt());
                })?;
            }
            UpdateBehavior::Delegate => {}
        }
        self.inner.update(id, f)
    }

    fn list(&self) -> Result<Vec<ProjectSummary>, StoreError> {
        self.inner.list()
    }

    fn patch_chapter(
        &self,
        id: &ProjectId,
        index: usize,
        receipt: ChapterReceipt,
    ) -> Result<(), StoreError> {
        self.inner.patch_chapter(id, index, receipt)
    }

    fn set_selection(&self, id: &ProjectId, skipped_ids: &[ChapterId]) -> Result<(), StoreError> {
        self.inner.set_selection(id, skipped_ids)
    }

    fn apply_mapping_op(
        &self,
        id: &ProjectId,
        op: MappingOp,
        expected_op_id: u64,
    ) -> Result<MappingState, StoreError> {
        self.inner.apply_mapping_op(id, op, expected_op_id)
    }
}

fn range(start: usize, end: usize) -> DetectedRange {
    DetectedRange {
        start_chapter_id: ChapterId::from_order(start),
        end_chapter_id: ChapterId::from_order(end),
    }
}

fn preview(range: DetectedRange) -> DetectionPreview {
    DetectionPreview {
        provider_id: Some(TranscribeProviderId::Groq),
        align_source: AlignSource::Transcript,
        range,
        confidence: 0.91,
        transcript_head_preview: Some("head preview".into()),
        transcript_tail_preview: Some("tail preview".into()),
        detected_at: Utc::now() - ChronoDuration::days(1),
        atom_starts: Vec::new(),
    }
}

fn interleaved_receipt() -> ChapterReceipt {
    ChapterReceipt {
        chapter_index: 0,
        track_index: Some(0),
        lesson_id: Some(42),
        degraded: false,
        uploaded_at: None,
    }
}

#[derive(Clone, Copy)]
enum ExpectedValidationError {
    DetectedRange,
    Unsupported,
}

async fn assert_confirm_rejected_unchanged(
    store: &UpdateProbeStore<'_>,
    project: Project,
    selected_range: DetectedRange,
    preview: DetectionPreview,
    label: &str,
    expected_error: ExpectedValidationError,
    message_fragment: &str,
) {
    store.put(&project).unwrap();
    let before = store.get(&project.id).unwrap().unwrap();
    let calls_before = store.update_calls();

    let error = confirm_detected_range_impl(
        store,
        &project.id,
        selected_range,
        preview,
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap_err();

    assert!(
        matches!(
            (expected_error, &error),
            (
                ExpectedValidationError::DetectedRange,
                AppError::DetectedRange(_)
            ) | (
                ExpectedValidationError::Unsupported,
                AppError::Unsupported(_)
            )
        ),
        "{label}: wrong validation error type: {error:?}"
    );
    assert!(
        error.to_string().contains(message_fragment),
        "{label}: error must contain '{message_fragment}', got {error}"
    );
    assert_eq!(store.update_calls(), calls_before, "{label}: no update");
    assert_eq!(
        store.get(&project.id).unwrap().unwrap(),
        before,
        "{label}: whole project unchanged"
    );
}

async fn assert_confirmed_condition(
    store: &UpdateProbeStore<'_>,
    chapter_count: usize,
    track_count: usize,
    expected_condition: MismatchCondition,
) {
    let mut fixture = groq_project(chapter_count, track_count);
    fixture.project.confirmed_at = Some(Utc::now());
    store.put(&fixture.project).unwrap();
    let selected_range = range(1, chapter_count - 2);
    let calls_before = store.update_calls();

    confirm_detected_range_impl(
        store,
        &fixture.project.id,
        selected_range.clone(),
        preview(range(0, chapter_count - 1)),
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap();

    assert_eq!(store.update_calls(), calls_before + 1);
    let project = store.get(&fixture.project.id).unwrap().unwrap();
    let decision = project.matcher_decision.unwrap();
    assert_eq!(decision.condition, expected_condition);
    assert_eq!(decision.chapter_count, chapter_count);
    assert_eq!(decision.track_count, track_count);
    assert_eq!(decision.response, MismatchResponse::SplitProportional);
    assert!(decision.user_overrode);
    assert_eq!(decision.detection.unwrap().range, selected_range);
    assert!(project.mapping.is_some());
    assert!(project.confirmed_at.is_none());
}

async fn run_detection_confirmation_reset_contract(store: &dyn ProjectStore) {
    let probe = UpdateProbeStore::new(store);

    let mut fixture = groq_project(6, 3);
    fixture.project.confirmed_at = Some(Utc::now());
    probe.put(&fixture.project).unwrap();
    let selected_range = range(2, 4);
    let transient_preview = preview(range(1, 5));
    let preview_detected_at = transient_preview.detected_at;
    let started_at = Utc::now();
    let calls_before = probe.update_calls();

    confirm_detected_range_impl(
        &probe,
        &fixture.project.id,
        selected_range.clone(),
        transient_preview,
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap();

    let finished_at = Utc::now();
    assert_eq!(probe.update_calls(), calls_before + 1);
    let confirmed = probe.get(&fixture.project.id).unwrap().unwrap();
    let decision = confirmed.matcher_decision.as_ref().unwrap();
    assert_eq!(decision.condition, MismatchCondition::ManyToFew);
    assert_eq!(decision.response, MismatchResponse::SplitProportional);
    assert_eq!(decision.chapter_count, 6);
    assert_eq!(decision.track_count, 3);
    assert!(!decision.user_overrode);
    assert!((started_at..=finished_at).contains(&decision.decided_at));
    let persisted_evidence = decision.detection.as_ref().unwrap();
    assert_eq!(persisted_evidence.range, selected_range);
    assert_ne!(persisted_evidence.detected_at, preview_detected_at);
    assert!((started_at..=finished_at).contains(&persisted_evidence.detected_at));
    assert_eq!(
        confirmed
            .mapping
            .as_ref()
            .unwrap()
            .pairs
            .iter()
            .map(|pair| pair.chapter_id.clone())
            .collect::<Vec<_>>(),
        vec![
            ChapterId::from_order(2),
            ChapterId::from_order(3),
            ChapterId::from_order(4),
        ]
    );
    assert!(confirmed.confirmed_at.is_none());

    confirm_mapping_impl(&probe, &fixture.project.id).unwrap();
    assert!(probe
        .get(&fixture.project.id)
        .unwrap()
        .unwrap()
        .confirmed_at
        .is_some());
    assert_eq!(
        plan_preview(&probe, &fixture.project.id)
            .await
            .unwrap()
            .iter()
            .map(|step| step.chapter_index)
            .collect::<Vec<_>>(),
        vec![2, 3, 4]
    );

    assert_eq!(
        allowed(MismatchCondition::ManyToOne),
        (
            vec![MismatchResponse::SingleLesson, MismatchResponse::Cancel],
            MismatchResponse::SingleLesson,
        )
    );
    assert_confirmed_condition(&probe, 5, 1, MismatchCondition::ManyToOne).await;
    assert_eq!(
        allowed(MismatchCondition::ManyToOne),
        (
            vec![MismatchResponse::SingleLesson, MismatchResponse::Cancel],
            MismatchResponse::SingleLesson,
        )
    );

    assert_eq!(
        allowed(MismatchCondition::Unalignable),
        (
            vec![MismatchResponse::SingleLesson, MismatchResponse::Cancel],
            MismatchResponse::Cancel,
        )
    );
    assert_confirmed_condition(&probe, 70, 2, MismatchCondition::Unalignable).await;
    assert_eq!(
        allowed(MismatchCondition::Unalignable),
        (
            vec![MismatchResponse::SingleLesson, MismatchResponse::Cancel],
            MismatchResponse::Cancel,
        )
    );

    let fixture = groq_project(6, 3);
    assert_confirm_rejected_unchanged(
        &probe,
        fixture.project,
        DetectedRange {
            start_chapter_id: ChapterId("missing".into()),
            end_chapter_id: ChapterId::from_order(4),
        },
        preview(range(1, 4)),
        "missing selected boundary",
        ExpectedValidationError::DetectedRange,
        "missing",
    )
    .await;

    let fixture = groq_project(6, 3);
    assert_confirm_rejected_unchanged(
        &probe,
        fixture.project,
        range(4, 1),
        preview(range(1, 4)),
        "reordered selected boundaries",
        ExpectedValidationError::DetectedRange,
        "precedes",
    )
    .await;

    let fixture = groq_project(6, 3);
    let mut invalid_preview = preview(range(1, 4));
    invalid_preview.range.start_chapter_id = ChapterId("missing-preview".into());
    assert_confirm_rejected_unchanged(
        &probe,
        fixture.project,
        range(1, 4),
        invalid_preview,
        "missing preview boundary",
        ExpectedValidationError::Unsupported,
        "text source changed",
    )
    .await;

    let fixture = groq_project(6, 3);
    assert_confirm_rejected_unchanged(
        &probe,
        fixture.project,
        range(1, 4),
        preview(range(4, 1)),
        "reordered preview boundaries",
        ExpectedValidationError::DetectedRange,
        "precedes",
    )
    .await;

    for (label, confidence) in [
        ("NaN confidence", f32::NAN),
        ("infinite confidence", f32::INFINITY),
        ("negative confidence", -0.01),
        ("confidence over one", 1.01),
    ] {
        let fixture = groq_project(6, 3);
        let mut invalid_preview = preview(range(1, 4));
        invalid_preview.confidence = confidence;
        assert_confirm_rejected_unchanged(
            &probe,
            fixture.project,
            range(1, 4),
            invalid_preview,
            label,
            ExpectedValidationError::Unsupported,
            "confidence",
        )
        .await;
    }

    for (label, align_source, provider_id) in [
        (
            "title evidence with provider",
            AlignSource::Title,
            Some(TranscribeProviderId::Groq),
        ),
        (
            "transcript evidence without provider",
            AlignSource::Transcript,
            None,
        ),
    ] {
        let fixture = groq_project(6, 3);
        let mut invalid_preview = preview(range(1, 4));
        invalid_preview.align_source = align_source;
        invalid_preview.provider_id = provider_id;
        assert_confirm_rejected_unchanged(
            &probe,
            fixture.project,
            range(1, 4),
            invalid_preview,
            label,
            ExpectedValidationError::Unsupported,
            "provider",
        )
        .await;
    }

    for (label, head_overlong) in [
        ("overlong head preview", true),
        ("overlong tail preview", false),
    ] {
        let fixture = groq_project(6, 3);
        let mut invalid_preview = preview(range(1, 4));
        if head_overlong {
            invalid_preview.transcript_head_preview = Some("界".repeat(241));
        } else {
            invalid_preview.transcript_tail_preview = Some("🙂".repeat(241));
        }
        assert_confirm_rejected_unchanged(
            &probe,
            fixture.project,
            range(1, 4),
            invalid_preview,
            label,
            ExpectedValidationError::Unsupported,
            "previews",
        )
        .await;
    }

    let fixture = groq_project(5, 3);
    assert_confirm_rejected_unchanged(
        &probe,
        fixture.project,
        range(1, 3),
        preview(range(1, 3)),
        "current condition is not detection eligible",
        ExpectedValidationError::Unsupported,
        "eligible",
    )
    .await;

    let fixture = groq_project(6, 3);
    let failing = UpdateProbeStore::failing(store);
    failing.put(&fixture.project).unwrap();
    let before = failing.get(&fixture.project.id).unwrap().unwrap();
    let error = confirm_detected_range_impl(
        &failing,
        &fixture.project.id,
        range(1, 4),
        preview(range(1, 4)),
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, AppError::Other(message) if message.contains("store.update")),
        "injected failure must identify the store update"
    );
    assert_eq!(failing.update_calls(), 1);
    assert_eq!(failing.get(&fixture.project.id).unwrap().unwrap(), before);

    let fixture = groq_project(6, 3);
    let interleaving = UpdateProbeStore::changing_selection(store);
    interleaving.put(&fixture.project).unwrap();
    let mut expected_after_interleave = fixture.project.clone();
    expected_after_interleave
        .skipped_chapters
        .push(ChapterId::from_order(0));
    let result = confirm_detected_range_impl(
        &interleaving,
        &fixture.project.id,
        range(1, 4),
        preview(range(1, 4)),
        TranscribeProviderId::Groq,
    )
    .await;
    assert!(
        matches!(result, Err(AppError::Unsupported(message)) if message.contains("changed")),
        "stale confirmation must return an actionable unsupported error"
    );
    assert_eq!(interleaving.update_calls(), 1);
    assert_eq!(
        interleaving.get(&fixture.project.id).unwrap().unwrap(),
        expected_after_interleave
    );

    let mut fixture = availability_project(6, 3);
    fixture.project.settings.tags = vec!["preserve-me".into()];
    fixture.project.cover_use = false;
    fixture.project.transcribe_consent = Some(TranscribeConsent {
        provider_id: TranscribeProviderId::Groq,
        accepted_at: Utc::now(),
    });
    probe.put(&fixture.project).unwrap();
    confirm_detected_range_impl(
        &probe,
        &fixture.project.id,
        range(1, 4),
        preview(range(1, 4)),
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap();
    store
        .update(&fixture.project.id, &mut |project| {
            project.confirmed_at = Some(Utc::now());
        })
        .unwrap();
    let before_reset = probe.get(&fixture.project.id).unwrap().unwrap();
    let calls_before = probe.update_calls();

    reset_detection_impl(&probe, &fixture.project.id).unwrap();

    assert_eq!(probe.update_calls(), calls_before + 1);
    let reset = probe.get(&fixture.project.id).unwrap().unwrap();
    let mut expected_reset = before_reset;
    expected_reset.matcher_decision = None;
    expected_reset.mapping = None;
    expected_reset.confirmed_at = None;
    assert_eq!(reset, expected_reset);

    probe.put(&fixture.project).unwrap();
    confirm_detected_range_impl(
        &probe,
        &fixture.project.id,
        range(1, 4),
        preview(range(1, 4)),
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap();
    store
        .update(&fixture.project.id, &mut |project| {
            project.receipts.push(ChapterReceipt {
                chapter_index: 0,
                track_index: Some(0),
                lesson_id: Some(42),
                degraded: false,
                uploaded_at: Some(Utc::now()),
            });
        })
        .unwrap();
    let before_receipt_reset = probe.get(&fixture.project.id).unwrap().unwrap();
    let calls_before = probe.update_calls();
    let error = reset_detection_impl(&probe, &fixture.project.id).unwrap_err();
    assert!(
        matches!(error, AppError::Unsupported(message) if message.contains("uploads")),
        "receipt reset must return an actionable unsupported error"
    );
    assert_eq!(probe.update_calls(), calls_before);
    assert_eq!(
        probe.get(&fixture.project.id).unwrap().unwrap(),
        before_receipt_reset
    );

    let mut no_evidence = fixture.project.clone();
    no_evidence.matcher_decision = Some(MatcherDecision {
        condition: MismatchCondition::ManyToFew,
        response: MismatchResponse::SplitProportional,
        chapter_count: 6,
        track_count: 3,
        user_overrode: false,
        decided_at: Utc::now(),
        detection: None,
    });
    no_evidence.mapping = Some(MappingState::default());
    no_evidence.confirmed_at = Some(Utc::now());
    probe.put(&no_evidence).unwrap();
    let before_no_evidence = probe.get(&no_evidence.id).unwrap().unwrap();
    let calls_before = probe.update_calls();
    let error = reset_detection_impl(&probe, &no_evidence.id).unwrap_err();
    assert!(
        matches!(error, AppError::Unsupported(message) if message.contains("no confirmed detection")),
        "reset without evidence must explain the missing confirmation"
    );
    assert_eq!(probe.update_calls(), calls_before);
    assert_eq!(
        probe.get(&no_evidence.id).unwrap().unwrap(),
        before_no_evidence
    );

    probe.put(&fixture.project).unwrap();
    confirm_detected_range_impl(
        &probe,
        &fixture.project.id,
        range(1, 4),
        preview(range(1, 4)),
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap();
    let interleaving = UpdateProbeStore::adding_receipt(store);
    let mut expected_after_interleave = interleaving.get(&fixture.project.id).unwrap().unwrap();
    expected_after_interleave
        .receipts
        .push(interleaved_receipt());
    let result = reset_detection_impl(&interleaving, &fixture.project.id);
    assert!(
        matches!(result, Err(AppError::Unsupported(message)) if message.contains("uploads")),
        "receipt interleave must return an actionable unsupported error"
    );
    assert_eq!(interleaving.update_calls(), 1);
    assert_eq!(
        interleaving.get(&fixture.project.id).unwrap().unwrap(),
        expected_after_interleave
    );

    probe.put(&fixture.project).unwrap();
    confirm_detected_range_impl(
        &probe,
        &fixture.project.id,
        range(1, 4),
        preview(range(1, 4)),
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap();
    let failing = UpdateProbeStore::failing(store);
    let before_failed_reset = failing.get(&fixture.project.id).unwrap().unwrap();
    let error = reset_detection_impl(&failing, &fixture.project.id).unwrap_err();
    assert!(
        matches!(error, AppError::Other(message) if message.contains("store.update")),
        "injected reset failure must identify the store update"
    );
    assert_eq!(failing.update_calls(), 1);
    assert_eq!(
        failing.get(&fixture.project.id).unwrap().unwrap(),
        before_failed_reset
    );
}

#[tokio::test]
async fn in_memory_store_passes_detection_confirmation_reset_contract() {
    let store = InMemoryProjectStore::new();
    run_detection_confirmation_reset_contract(&store).await;
}

#[tokio::test]
async fn json_store_passes_detection_confirmation_reset_contract() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(dir.path());
    run_detection_confirmation_reset_contract(&store).await;
}

#[tokio::test]
async fn collapsed_range_confirm_packs_all_eligible_chapters() {
    let store = InMemoryProjectStore::new();
    let probe = UpdateProbeStore::new(&store);
    let fixture = groq_project(6, 3);
    probe.put(&fixture.project).unwrap();
    let selected = range(2, 2);

    confirm_detected_range_impl(
        &probe,
        &fixture.project.id,
        selected.clone(),
        preview(selected.clone()),
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap();

    let project = probe.get(&fixture.project.id).unwrap().unwrap();
    let mapping = project.mapping.expect("mapping seeded");
    assert_eq!(
        mapping.pairs.len(),
        6,
        "degenerate start=end must keep every eligible chapter, got {mapping:?}"
    );
    assert_eq!(
        project.matcher_decision.unwrap().detection.unwrap().range,
        selected
    );
}

#[tokio::test]
async fn atom_start_confirm_packs_chapters_between_audio_parts() {
    let store = InMemoryProjectStore::new();
    let probe = UpdateProbeStore::new(&store);
    let fixture = groq_project(6, 3);
    probe.put(&fixture.project).unwrap();
    let selected = range(0, 5);
    let mut preview = preview(selected.clone());
    preview.atom_starts = vec![
        AtomStart {
            track_index: 0,
            chapter_id: ChapterId::from_order(0),
        },
        AtomStart {
            track_index: 1,
            chapter_id: ChapterId::from_order(1),
        },
        AtomStart {
            track_index: 2,
            chapter_id: ChapterId::from_order(4),
        },
    ];

    confirm_detected_range_impl(
        &probe,
        &fixture.project.id,
        selected,
        preview,
        TranscribeProviderId::Groq,
    )
    .await
    .unwrap();

    let mapping = probe
        .get(&fixture.project.id)
        .unwrap()
        .unwrap()
        .mapping
        .expect("mapping seeded");
    let tracks: Vec<_> = mapping
        .pairs
        .iter()
        .map(|pair| pair.track_id.clone())
        .collect();
    assert_eq!(mapping.pairs.len(), 6);
    assert_eq!(tracks[0], tracks[0]);
    assert_ne!(tracks[0], tracks[1]);
    assert_eq!(tracks[1], tracks[2]);
    assert_eq!(tracks[2], tracks[3]);
    assert_ne!(tracks[3], tracks[4]);
    assert_eq!(tracks[4], tracks[5]);
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

#[derive(Clone, Copy)]
enum LocalAction {
    /// Refining a preview boundary and confirming it — a local decision that
    /// must never re-upload audio.
    Refine,
    Reset,
}

struct AutoCase {
    name: &'static str,
    fixture: AvailabilityFixture,
    active_provider: TranscribeProviderId,
    key: Option<&'static str>,
    action: Option<LocalAction>,
    /// A reset is not itself a trigger: the one-shot suppression that keeps the
    /// next mount from restarting lives in the resolver and is covered by the
    /// browser spec, so a reset row only proves the reset touched no provider.
    reconsiders_start: bool,
}

fn auto_case(name: &'static str, fixture: AvailabilityFixture) -> AutoCase {
    AutoCase {
        name,
        fixture,
        active_provider: TranscribeProviderId::Groq,
        key: Some("groq-test-key"),
        action: None,
        reconsiders_start: true,
    }
}

#[derive(Debug)]
struct AutoCounters {
    factory_calls: usize,
    transcribe_calls: usize,
    started: bool,
    /// `None` when availability itself refused to answer.
    can_start: Option<bool>,
}

/// Replays one auto-mode consideration end to end: optional local action, then
/// the `can_start` gate, then — only if the gate opens — a real detection run
/// against counting fakes.
async fn run_auto_consideration(case: AutoCase) -> AutoCounters {
    let preferences_dir = tempfile::tempdir().unwrap();
    write_preferences(preferences_dir.path(), case.active_provider);
    let store = InMemoryProjectStore::new();
    store.put(&case.fixture.project).unwrap();
    let project_id = case.fixture.project.id.clone();

    match case.action {
        Some(LocalAction::Refine) => {
            confirm_detected_range_impl(
                &store,
                &project_id,
                range(2, 3),
                preview(range(1, 4)),
                TranscribeProviderId::Groq,
            )
            .await
            .unwrap_or_else(|error| panic!("{}: refine confirm failed: {error}", case.name));
        }
        Some(LocalAction::Reset) => {
            reset_detection_impl(&store, &project_id)
                .unwrap_or_else(|error| panic!("{}: reset failed: {error}", case.name));
        }
        None => {}
    }

    let project = store.get(&project_id).unwrap().unwrap();
    // An availability error is itself a refusal to start, so it counts as a
    // closed gate rather than a panic — the row still asserts zero uploads.
    let can_start = detection_availability_impl(&project, case.active_provider, case.key.is_some())
        .await
        .ok()
        .map(|availability| availability.can_start);
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let transcribe_calls = Arc::new(AtomicUsize::new(0));
    let mut started = false;

    if case.reconsiders_start && can_start == Some(true) {
        let creations = Arc::clone(&factory_calls);
        let calls = Arc::clone(&transcribe_calls);
        let key = case.key;
        let factory = detection_provider_factory(
            &store,
            &project_id,
            preferences_dir.path(),
            move |_| {
                key.map(SecretString::from).ok_or_else(|| {
                    AppError::Transcribe(TranscribeError {
                        kind: TranscribeErrorKind::ApiKey,
                        message: "no fake key".into(),
                    })
                })
            },
            move |_, _| {
                creations.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(CountingTranscriber { calls }))
            },
        );
        let mut sink = RecordingDetectionSink::default();
        let _ = detect_start_offset_impl(
            &project,
            &empty_cancel_map(),
            Uuid::new_v4(),
            &mut sink,
            factory,
        )
        .await;
        started = true;
    }

    AutoCounters {
        factory_calls: factory_calls.load(Ordering::SeqCst),
        transcribe_calls: transcribe_calls.load(Ordering::SeqCst),
        started,
        can_start,
    }
}

fn privacy_cases() -> Vec<AutoCase> {
    vec![
        auto_case("missing consent", availability_project(6, 3)),
        AutoCase {
            active_provider: TranscribeProviderId::OpenAi,
            key: Some("openai-test-key"),
            ..auto_case("groq consent after switching to openai", groq_project(6, 3))
        },
        AutoCase {
            key: None,
            ..auto_case("missing selected provider key", groq_project(6, 3))
        },
        auto_case("count off", groq_project(5, 3)),
        auto_case("one to many", groq_project(1, 3)),
        auto_case("zero tracks", groq_project(6, 0)),
        auto_case("zero chapters", groq_project(0, 3)),
        auto_case("tracks heavy mismatch", groq_project(2, 7)),
        auto_case("existing evidence", with_evidence(groq_project(6, 3), 6, 3)),
        AutoCase {
            action: Some(LocalAction::Refine),
            ..auto_case("local preview refine", groq_project(6, 3))
        },
        AutoCase {
            action: Some(LocalAction::Reset),
            reconsiders_start: false,
            ..auto_case("reset command", with_evidence(groq_project(6, 3), 6, 3))
        },
    ]
}

#[tokio::test]
async fn privacy_matrix_uploads_nothing_until_an_eligible_trigger() {
    for case in privacy_cases() {
        let name = case.name;
        let reconsiders_start = case.reconsiders_start;
        let counters = run_auto_consideration(case).await;
        assert_eq!(counters.factory_calls, 0, "{name} constructed provider");
        assert_eq!(counters.transcribe_calls, 0, "{name} uploaded audio");
        assert!(!counters.started, "{name} started a detection job");
        if reconsiders_start {
            assert_eq!(
                counters.can_start,
                Some(false),
                "{name} must be refused by the availability gate"
            );
        }
    }
}

#[tokio::test]
async fn privacy_matrix_lets_an_explicit_rerun_after_reset_start_one_job() {
    let counters = run_auto_consideration(AutoCase {
        action: Some(LocalAction::Reset),
        ..auto_case(
            "explicit rerun after reset",
            with_evidence(
                consented(stage_b_project(), TranscribeProviderId::Groq),
                6,
                1,
            ),
        )
    })
    .await;

    assert!(counters.started);
    assert_eq!(counters.factory_calls, 1);
    assert_eq!(counters.transcribe_calls, 2);
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
