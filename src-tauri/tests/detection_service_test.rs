use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use lingq_upload_lib::core::audio::AudioTrack;
use lingq_upload_lib::core::epub::{Chapter, ChapterId};
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::events::DetectionPhase;
use lingq_upload_lib::transcribe::{
    detect_start_offset, AlignSource, AlignmentConfig, DetectStartResult, DetectedRange,
    DetectionSink, NoTranscriptReason, ProviderFactory, TranscribeError, TranscribeErrorKind,
    TranscribeOpts, TranscribeProviderId, Transcriber, Transcript,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const HEAD_TEXT: &str =
    "Homeward bound after many years away she finally saw the harbor lights again.";
const TAIL_TEXT: &str = "Years later the same harbor lights guided travelers home through the fog.";

trait RangeConsumer {
    fn consume(&mut self, range: &DetectedRange, ordered_ids: &[ChapterId]);
}

struct RecordingRangeConsumer {
    range: DetectedRange,
    ordered_ids: Vec<ChapterId>,
}

impl RangeConsumer for RecordingRangeConsumer {
    fn consume(&mut self, range: &DetectedRange, ordered_ids: &[ChapterId]) {
        self.range = range.clone();
        self.ordered_ids = ordered_ids.to_vec();
    }
}

fn consume_detected_range(
    result: &DetectStartResult,
    chapters: &[Chapter],
    consumer: &mut dyn RangeConsumer,
) {
    let DetectStartResult::Detected { preview } = result else {
        return;
    };
    let Some(start) = chapters
        .iter()
        .position(|chapter| chapter.id == preview.range.start_chapter_id)
    else {
        return;
    };
    let Some(end) = chapters
        .iter()
        .position(|chapter| chapter.id == preview.range.end_chapter_id)
    else {
        return;
    };
    if start > end {
        return;
    }
    let ordered_ids: Vec<_> = chapters[start..=end]
        .iter()
        .map(|chapter| chapter.id.clone())
        .collect();
    consumer.consume(&preview.range, &ordered_ids);
}

#[derive(Clone, Debug, PartialEq)]
enum RecordedEvent {
    Started,
    Progress(f32, DetectionPhase),
    Result,
    Error(TranscribeErrorKind),
    Cancelled,
}

#[derive(Default)]
struct RecordingDetectionSink {
    events: Vec<RecordedEvent>,
    cancel_on_phase: Option<(DetectionPhase, CancellationToken)>,
}

impl RecordingDetectionSink {
    fn cancelling(phase: DetectionPhase, cancel: CancellationToken) -> Self {
        Self {
            events: Vec::new(),
            cancel_on_phase: Some((phase, cancel)),
        }
    }

    fn terminal_count(&self) -> usize {
        self.events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    RecordedEvent::Result | RecordedEvent::Error(_) | RecordedEvent::Cancelled
                )
            })
            .count()
    }
}

impl DetectionSink for RecordingDetectionSink {
    fn started(&mut self, _: Uuid) {
        self.events.push(RecordedEvent::Started);
    }

    fn progress(&mut self, _: Uuid, pct: f32, phase: DetectionPhase) {
        self.events.push(RecordedEvent::Progress(pct, phase));
        if let Some((cancel_phase, cancel)) = &self.cancel_on_phase {
            if *cancel_phase == phase {
                cancel.cancel();
            }
        }
    }

    fn result(&mut self, _: Uuid, _: &DetectStartResult) {
        self.events.push(RecordedEvent::Result);
    }

    fn error(&mut self, _: Uuid, error: &TranscribeError) {
        self.events.push(RecordedEvent::Error(error.kind()));
    }

    fn cancelled(&mut self, _: Uuid) {
        self.events.push(RecordedEvent::Cancelled);
    }
}

struct ScriptedTranscriber {
    responses: Mutex<VecDeque<Result<Transcript, TranscribeError>>>,
    paths: Arc<Mutex<Vec<PathBuf>>>,
    cancel_on_call: Option<(usize, CancellationToken)>,
}

impl Transcriber for ScriptedTranscriber {
    fn provider_id(&self) -> TranscribeProviderId {
        TranscribeProviderId::Groq
    }

    fn transcribe(
        &self,
        audio: &Path,
        _: &TranscribeOpts,
    ) -> BoxFuture<'_, Result<Transcript, TranscribeError>> {
        let mut paths = self.paths.lock().unwrap();
        paths.push(audio.to_owned());
        let call = paths.len();
        drop(paths);
        if let Some((cancel_call, cancel)) = &self.cancel_on_call {
            if *cancel_call == call {
                cancel.cancel();
            }
        }
        let response = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted response for every provider call");
        Box::pin(async move { response })
    }
}

fn transcript(text: impl Into<String>) -> Result<Transcript, TranscribeError> {
    Ok(Transcript { text: text.into() })
}

fn error(kind: TranscribeErrorKind) -> TranscribeError {
    TranscribeError {
        kind,
        message: format!("scripted {kind:?}"),
    }
}

fn counting_factory(
    factory_calls: Arc<AtomicUsize>,
    paths: Arc<Mutex<Vec<PathBuf>>>,
    responses: Vec<Result<Transcript, TranscribeError>>,
) -> ProviderFactory<'static> {
    scripted_factory(factory_calls, paths, responses, None)
}

fn scripted_factory(
    factory_calls: Arc<AtomicUsize>,
    paths: Arc<Mutex<Vec<PathBuf>>>,
    responses: Vec<Result<Transcript, TranscribeError>>,
    cancel_on_call: Option<(usize, CancellationToken)>,
) -> ProviderFactory<'static> {
    Box::new(move || {
        factory_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(ScriptedTranscriber {
            responses: Mutex::new(responses.into()),
            paths,
            cancel_on_call,
        }))
    })
}

fn failing_factory(
    factory_calls: Arc<AtomicUsize>,
    kind: TranscribeErrorKind,
) -> ProviderFactory<'static> {
    Box::new(move || {
        factory_calls.fetch_add(1, Ordering::SeqCst);
        Err(error(kind))
    })
}

fn project() -> Project {
    Project::new_test(
        ProjectId::from_title_author("detection", "test"),
        "Detection",
    )
}

fn chapters() -> Vec<Chapter> {
    [
        (
            "Prologue",
            "Once upon a quiet morning in the village the bells rang softly over the hills.",
        ),
        (
            "Valley",
            "The wind swept through the valley of stone and scattered dust across the road.",
        ),
        ("Return", HEAD_TEXT),
        ("Epilogue", TAIL_TEXT),
    ]
    .into_iter()
    .enumerate()
    .map(|(order, (title, body))| Chapter {
        order,
        title: title.into(),
        body: body.into(),
        id: ChapterId::from_chapter_parts("test", &format!("spine-{order}"), title),
        ..Default::default()
    })
    .collect()
}

fn fixture_track(window: Option<(f64, f64)>) -> AudioTrack {
    AudioTrack {
        order: 0,
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/probe_3min.mp3"),
        duration_sec: None,
        title: None,
        window,
    }
}

async fn run(
    tracks: &[AudioTrack],
    chapters: &[Chapter],
    sink: &mut RecordingDetectionSink,
    cancel: CancellationToken,
    factory: ProviderFactory<'_>,
) -> Result<DetectStartResult, TranscribeError> {
    detect_start_offset(
        &project(),
        tracks,
        chapters,
        Uuid::new_v4(),
        &AlignmentConfig::default(),
        sink,
        cancel,
        factory,
    )
    .await
}

#[tokio::test]
async fn stage_a_does_not_touch_audio_or_construct_provider() {
    let chapters = chapters();
    let tracks = vec![
        AudioTrack {
            order: 0,
            path: PathBuf::from("missing-head.mp3"),
            duration_sec: None,
            title: Some("Return".into()),
            window: None,
        },
        AudioTrack {
            order: 1,
            path: PathBuf::from("missing-tail.mp3"),
            duration_sec: None,
            title: Some("Epilogue".into()),
            window: None,
        },
    ];
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let mut sink = RecordingDetectionSink::default();

    let result = run(
        &tracks,
        &chapters,
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::clone(&factory_calls),
            Arc::new(Mutex::new(Vec::new())),
            Vec::new(),
        ),
    )
    .await
    .unwrap();

    let DetectStartResult::Detected { preview } = result else {
        panic!("expected title detection");
    };
    assert_eq!(preview.provider_id, None);
    assert_eq!(preview.align_source, AlignSource::Title);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        sink.events,
        [
            RecordedEvent::Started,
            RecordedEvent::Progress(0.05, DetectionPhase::TitleCheck),
            RecordedEvent::Result,
        ]
    );
}

#[tokio::test]
async fn stage_b_constructs_one_provider_runs_head_then_tail_and_cleans_samples() {
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let paths = Arc::new(Mutex::new(Vec::new()));
    let mut sink = RecordingDetectionSink::default();

    let result = run(
        &[fixture_track(None)],
        &chapters(),
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::clone(&factory_calls),
            Arc::clone(&paths),
            vec![transcript(HEAD_TEXT), transcript(TAIL_TEXT)],
        ),
    )
    .await
    .unwrap();

    assert!(matches!(result, DetectStartResult::Detected { .. }));
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    let paths = paths.lock().unwrap();
    assert_eq!(
        paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["head-0.mp3", "tail-0.mp3"]
    );
    assert!(paths.iter().all(|path| !path.exists()));

    let progress: Vec<_> = sink
        .events
        .iter()
        .filter_map(|event| match event {
            RecordedEvent::Progress(pct, phase) => Some((*pct, *phase)),
            _ => None,
        })
        .collect();
    assert_eq!(
        progress,
        [
            (0.05, DetectionPhase::TitleCheck),
            (0.15, DetectionPhase::SampleHead),
            (0.30, DetectionPhase::TranscribeHead),
            (0.45, DetectionPhase::AlignHead),
            (0.55, DetectionPhase::SampleTail),
            (0.70, DetectionPhase::TranscribeTail),
            (0.90, DetectionPhase::AlignTail),
        ]
    );
    assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert_eq!(sink.events.first(), Some(&RecordedEvent::Started));
    assert_eq!(sink.events.last(), Some(&RecordedEvent::Result));
    assert_eq!(sink.terminal_count(), 1);
}

#[tokio::test]
async fn content_poor_head_and_tail_retry_once_for_four_calls_maximum() {
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let paths = Arc::new(Mutex::new(Vec::new()));
    let mut sink = RecordingDetectionSink::default();

    let result = run(
        &[fixture_track(None)],
        &chapters(),
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::clone(&factory_calls),
            Arc::clone(&paths),
            vec![
                transcript("too short"),
                transcript(HEAD_TEXT),
                transcript("also short"),
                transcript(TAIL_TEXT),
            ],
        ),
    )
    .await
    .unwrap();

    assert!(matches!(result, DetectStartResult::Detected { .. }));
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        paths
            .lock()
            .unwrap()
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["head-0.mp3", "head-1.mp3", "tail-0.mp3", "tail-1.mp3"]
    );
    assert_eq!(sink.terminal_count(), 1);
}

#[tokio::test]
async fn exhausted_or_unavailable_retry_returns_typed_content_outcome() {
    for (window, responses, expected) in [
        (
            None,
            vec![transcript("short"), transcript("still short")],
            NoTranscriptReason::ContentPoor,
        ),
        (
            Some((0.0, 30.0)),
            vec![transcript("")],
            NoTranscriptReason::Empty,
        ),
    ] {
        let paths = Arc::new(Mutex::new(Vec::new()));
        let mut sink = RecordingDetectionSink::default();
        let result = run(
            &[fixture_track(window)],
            &chapters(),
            &mut sink,
            CancellationToken::new(),
            counting_factory(Arc::new(AtomicUsize::new(0)), Arc::clone(&paths), responses),
        )
        .await
        .unwrap();

        assert_eq!(result, DetectStartResult::NoTranscript { reason: expected });
        assert!(paths.lock().unwrap().len() <= 2);
        assert_eq!(sink.events.last(), Some(&RecordedEvent::Result));
        assert_eq!(sink.terminal_count(), 1);
    }
}

#[tokio::test]
async fn no_tracks_and_short_audio_finish_as_content_without_a_provider() {
    for (tracks, expected) in [
        (Vec::new(), NoTranscriptReason::Empty),
        (
            vec![fixture_track(Some((0.0, 29.999)))],
            NoTranscriptReason::InsufficientAudio,
        ),
    ] {
        let factory_calls = Arc::new(AtomicUsize::new(0));
        let mut sink = RecordingDetectionSink::default();
        let result = run(
            &tracks,
            &chapters(),
            &mut sink,
            CancellationToken::new(),
            counting_factory(
                Arc::clone(&factory_calls),
                Arc::new(Mutex::new(Vec::new())),
                Vec::new(),
            ),
        )
        .await
        .unwrap();

        assert_eq!(result, DetectStartResult::NoTranscript { reason: expected });
        assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
        assert_eq!(sink.terminal_count(), 1);
    }
}

#[tokio::test]
async fn ambiguity_returns_bounded_low_confidence_candidates() {
    let paths = Arc::new(Mutex::new(Vec::new()));
    let mut sink = RecordingDetectionSink::default();
    let unrelated_head =
        "Unrelated spoken words continue for long enough to be useful but match no chapter.";
    let unrelated_tail =
        "Another unrelated passage continues for long enough to match no ending chapter.";

    let result = run(
        &[fixture_track(None)],
        &chapters(),
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            paths,
            vec![transcript(unrelated_head), transcript(unrelated_tail)],
        ),
    )
    .await
    .unwrap();

    let DetectStartResult::LowConfidence {
        transcript_head_preview,
        transcript_tail_preview,
        top_head,
        top_tail,
    } = result
    else {
        panic!("expected low confidence");
    };
    assert_eq!(transcript_head_preview.as_deref(), Some(unrelated_head));
    assert_eq!(transcript_tail_preview.as_deref(), Some(unrelated_tail));
    assert_eq!(top_head.len(), 3);
    assert_eq!(top_tail.len(), 3);
    assert_eq!(sink.terminal_count(), 1);
}

#[tokio::test]
async fn operational_errors_remain_typed_and_emit_one_error_terminal() {
    for kind in [
        TranscribeErrorKind::Unauthorized,
        TranscribeErrorKind::RateLimit,
        TranscribeErrorKind::Timeout,
        TranscribeErrorKind::Network,
        TranscribeErrorKind::ProviderFailed,
    ] {
        let mut sink = RecordingDetectionSink::default();
        let result = run(
            &[fixture_track(None)],
            &chapters(),
            &mut sink,
            CancellationToken::new(),
            counting_factory(
                Arc::new(AtomicUsize::new(0)),
                Arc::new(Mutex::new(Vec::new())),
                vec![Err(error(kind))],
            ),
        )
        .await;

        assert_eq!(result.unwrap_err().kind(), kind);
        assert_eq!(sink.events.last(), Some(&RecordedEvent::Error(kind)));
        assert_eq!(sink.terminal_count(), 1);
    }

    let mut sink = RecordingDetectionSink::default();
    let result = run(
        &[fixture_track(None)],
        &chapters(),
        &mut sink,
        CancellationToken::new(),
        failing_factory(Arc::new(AtomicUsize::new(0)), TranscribeErrorKind::ApiKey),
    )
    .await;
    assert_eq!(result.unwrap_err().kind(), TranscribeErrorKind::ApiKey);
    assert_eq!(
        sink.events.last(),
        Some(&RecordedEvent::Error(TranscribeErrorKind::ApiKey))
    );
    assert_eq!(sink.terminal_count(), 1);

    let mut sink = RecordingDetectionSink::default();
    let missing = AudioTrack {
        path: PathBuf::from("definitely-missing.mp3"),
        ..fixture_track(None)
    };
    let result = run(
        &[missing],
        &chapters(),
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            Vec::new(),
        ),
    )
    .await;
    assert_eq!(result.unwrap_err().kind(), TranscribeErrorKind::Audio);
    assert_eq!(
        sink.events.last(),
        Some(&RecordedEvent::Error(TranscribeErrorKind::Audio))
    );
    assert_eq!(sink.terminal_count(), 1);
}

#[tokio::test]
async fn cancellation_at_every_phase_stops_future_provider_calls() {
    for (phase, expected_calls) in [
        (DetectionPhase::TitleCheck, 0),
        (DetectionPhase::SampleHead, 0),
        (DetectionPhase::TranscribeHead, 0),
        (DetectionPhase::AlignHead, 1),
        (DetectionPhase::SampleTail, 1),
        (DetectionPhase::TranscribeTail, 1),
        (DetectionPhase::AlignTail, 2),
    ] {
        let cancel = CancellationToken::new();
        let paths = Arc::new(Mutex::new(Vec::new()));
        let mut sink = RecordingDetectionSink::cancelling(phase, cancel.clone());
        let result = run(
            &[fixture_track(None)],
            &chapters(),
            &mut sink,
            cancel,
            counting_factory(
                Arc::new(AtomicUsize::new(0)),
                Arc::clone(&paths),
                vec![transcript(HEAD_TEXT), transcript(TAIL_TEXT)],
            ),
        )
        .await;

        assert!(result.is_err(), "phase {phase:?}");
        assert_eq!(
            paths.lock().unwrap().len(),
            expected_calls,
            "phase {phase:?}"
        );
        assert_eq!(sink.events.last(), Some(&RecordedEvent::Cancelled));
        assert_eq!(sink.terminal_count(), 1);
    }
}

#[tokio::test]
async fn cancellation_after_a_request_discards_its_result_before_retry_or_align() {
    let cancel = CancellationToken::new();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let mut sink = RecordingDetectionSink::default();
    let result = run(
        &[fixture_track(None)],
        &chapters(),
        &mut sink,
        cancel.clone(),
        scripted_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::clone(&paths),
            vec![transcript("short"), transcript(HEAD_TEXT)],
            Some((1, cancel)),
        ),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(paths.lock().unwrap().len(), 1);
    assert!(!sink
        .events
        .iter()
        .any(|event| { matches!(event, RecordedEvent::Progress(_, DetectionPhase::AlignHead)) }));
    assert_eq!(sink.events.last(), Some(&RecordedEvent::Cancelled));
    assert_eq!(sink.terminal_count(), 1);
}

#[tokio::test]
async fn detected_previews_are_unicode_safe_and_bounded_to_240_scalars() {
    let head = format!("{HEAD_TEXT}{}", "🦀".repeat(300));
    let tail = format!("{TAIL_TEXT}{}", "🌊".repeat(300));
    let mut sink = RecordingDetectionSink::default();

    let result = run(
        &[fixture_track(None)],
        &chapters(),
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            vec![transcript(head), transcript(tail)],
        ),
    )
    .await
    .unwrap();

    let DetectStartResult::Detected { preview } = result else {
        panic!("expected transcript detection");
    };
    let head = preview.transcript_head_preview.unwrap();
    let tail = preview.transcript_tail_preview.unwrap();
    assert_eq!(head.chars().count(), 240);
    assert_eq!(tail.chars().count(), 240);
    assert!(head.ends_with('🦀'));
    assert!(tail.ends_with('🌊'));
    assert_eq!(sink.terminal_count(), 1);
}

#[tokio::test]
async fn walking_skeleton_delivers_the_detected_inclusive_range() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let copied_audio = fixture_dir.path().join("walking-skeleton.mp3");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/probe_3min.mp3"),
        &copied_audio,
    )
    .unwrap();

    let bodies = [
        "Morning frost covered the empty orchard before the first cart arrived.",
        "A narrow path climbed from the mill toward a row of quiet cedar trees.",
        "At the stone bridge Lina opened the blue letter and read every careful line.",
        "The market clock paused at noon while rain passed over the tiled roofs.",
        "Beyond the station a gardener tied young branches against the northern wind.",
        "The final ferry crossed the silver inlet and reached the lantern pier at dusk.",
    ];
    let chapters: Vec<_> = bodies
        .into_iter()
        .enumerate()
        .map(|(order, body)| Chapter {
            order,
            id: ChapterId::from_order(order),
            title: format!("Section {}", order + 1),
            body: body.into(),
            ..Default::default()
        })
        .collect();
    let track = AudioTrack {
        order: 0,
        path: copied_audio,
        duration_sec: None,
        title: Some("Track 1".into()),
        window: None,
    };
    let mut sink = RecordingDetectionSink::default();
    let result = run(
        &[track],
        &chapters,
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            vec![transcript(bodies[2]), transcript(bodies[5])],
        ),
    )
    .await
    .unwrap();
    let mut consumer = RecordingRangeConsumer {
        range: DetectedRange {
            start_chapter_id: ChapterId::from_order(0),
            end_chapter_id: ChapterId::from_order(0),
        },
        ordered_ids: Vec::new(),
    };

    consume_detected_range(&result, &chapters, &mut consumer);

    assert_eq!(
        consumer.range,
        DetectedRange {
            start_chapter_id: ChapterId("idx:2".into()),
            end_chapter_id: ChapterId("idx:5".into()),
        }
    );
    assert_eq!(
        consumer.ordered_ids,
        vec![
            ChapterId::from_order(2),
            ChapterId::from_order(3),
            ChapterId::from_order(4),
            ChapterId::from_order(5),
        ]
    );
}

#[tokio::test]
async fn same_id_confident_span_is_low_confidence_when_other_chapters_exist() {
    let mut sink = RecordingDetectionSink::default();
    let result = run(
        &[fixture_track(None)],
        &chapters(),
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            vec![transcript(HEAD_TEXT), transcript(HEAD_TEXT)],
        ),
    )
    .await
    .unwrap();

    assert!(
        matches!(result, DetectStartResult::LowConfidence { .. }),
        "collapsed start=end must not be Detected, got {result:?}"
    );
}

#[tokio::test]
async fn single_chapter_book_may_detect_the_same_id_range() {
    let chapters = vec![Chapter {
        order: 0,
        title: "Return".into(),
        body: HEAD_TEXT.into(),
        id: ChapterId::from_chapter_parts("test", "spine-0", "Return"),
        ..Default::default()
    }];
    let mut sink = RecordingDetectionSink::default();
    let result = run(
        &[fixture_track(None)],
        &chapters,
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            vec![transcript(HEAD_TEXT), transcript(HEAD_TEXT)],
        ),
    )
    .await
    .unwrap();

    let DetectStartResult::Detected { preview } = result else {
        panic!("single eligible chapter must stay Detected, got {result:?}");
    };
    assert_eq!(preview.range.start_chapter_id, chapters[0].id);
    assert_eq!(preview.range.end_chapter_id, chapters[0].id);
}

#[tokio::test]
async fn multi_track_samples_each_atom_head_and_records_starts() {
    let chapters = chapters();
    let tracks = vec![
        fixture_track_at(0, Some((0.0, 60.0))),
        fixture_track_at(1, Some((60.0, 120.0))),
        fixture_track_at(2, Some((120.0, 180.0))),
    ];
    let paths = Arc::new(Mutex::new(Vec::new()));
    let mut sink = RecordingDetectionSink::default();
    let result = run(
        &tracks,
        &chapters,
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::clone(&paths),
            vec![
                transcript(chapters[0].body.clone()),
                transcript(chapters[1].body.clone()),
                transcript(chapters[2].body.clone()),
            ],
        ),
    )
    .await
    .unwrap();

    let DetectStartResult::Detected { preview } = result else {
        panic!("per-atom heads must detect, got {result:?}");
    };
    let names: Vec<_> = paths
        .lock()
        .unwrap()
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["head-0.mp3", "head-0.mp3", "head-0.mp3"]);
    assert_eq!(
        preview
            .atom_starts
            .iter()
            .map(|start| (start.track_index, start.chapter_id.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, chapters[0].id.clone()),
            (1, chapters[1].id.clone()),
            (2, chapters[2].id.clone()),
        ]
    );
    assert_eq!(preview.range.start_chapter_id, chapters[0].id);
    assert_eq!(preview.range.end_chapter_id, chapters[3].id);
}

#[tokio::test]
async fn multi_track_keeps_best_hit_per_atom_when_titles_collide() {
    let bodies = [
        "Once upon a quiet morning in the village the bells rang softly over the hills.",
        "The wind swept through the valley of stone and scattered dust across the road.",
        HEAD_TEXT,
        TAIL_TEXT,
    ];
    let chapters: Vec<_> = bodies
        .into_iter()
        .enumerate()
        .map(|(order, body)| Chapter {
            order,
            title: "時をかける少女".into(),
            body: body.into(),
            id: ChapterId::from_chapter_parts("test", &format!("spine-{order}"), body),
            ..Default::default()
        })
        .collect();
    let tracks = vec![
        fixture_track_at(0, Some((0.0, 60.0))),
        fixture_track_at(1, Some((60.0, 120.0))),
        fixture_track_at(2, Some((120.0, 180.0))),
    ];
    let mut sink = RecordingDetectionSink::default();
    let result = run(
        &tracks,
        &chapters,
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            vec![
                transcript(bodies[0]),
                transcript(bodies[1]),
                transcript(bodies[2]),
            ],
        ),
    )
    .await
    .unwrap();

    let DetectStartResult::Detected { preview } = result else {
        panic!("best per-atom hits must still detect when titles collide, got {result:?}");
    };
    assert_eq!(
        preview
            .atom_starts
            .iter()
            .map(|start| (start.track_index, start.chapter_id.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, chapters[0].id.clone()),
            (1, chapters[1].id.clone()),
            (2, chapters[2].id.clone()),
        ]
    );
}

#[tokio::test]
async fn multi_track_interior_clip_maps_the_chapter_that_contains_it() {
    let interior =
        "Then the narrow bridge groaned under the cart wheels as dawn broke over the ridge.";
    let valley = format!("{} {interior}", "paddingword ".repeat(50));
    let chapters = [
        (
            "Prologue",
            "Once upon a quiet morning in the village the bells rang softly over the hills."
                .to_string(),
        ),
        ("Valley", valley),
        ("Return", HEAD_TEXT.to_string()),
        ("Epilogue", TAIL_TEXT.to_string()),
    ]
    .into_iter()
    .enumerate()
    .map(|(order, (title, body))| Chapter {
        order,
        title: title.into(),
        body,
        id: ChapterId::from_chapter_parts("test", &format!("spine-{order}"), title),
        ..Default::default()
    })
    .collect::<Vec<_>>();
    let tracks = vec![
        fixture_track_at(0, Some((0.0, 60.0))),
        fixture_track_at(1, Some((60.0, 120.0))),
        fixture_track_at(2, Some((120.0, 180.0))),
    ];
    let mut sink = RecordingDetectionSink::default();
    let result = run(
        &tracks,
        &chapters,
        &mut sink,
        CancellationToken::new(),
        counting_factory(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            vec![
                transcript(chapters[0].body.clone()),
                transcript(interior),
                transcript(HEAD_TEXT),
            ],
        ),
    )
    .await
    .unwrap();

    let DetectStartResult::Detected { preview } = result else {
        panic!("interior atom clip must detect, got {result:?}");
    };
    assert_eq!(
        preview
            .atom_starts
            .iter()
            .map(|start| start.chapter_id.clone())
            .collect::<Vec<_>>(),
        vec![
            chapters[0].id.clone(),
            chapters[1].id.clone(),
            chapters[2].id.clone(),
        ]
    );
}

fn fixture_track_at(order: usize, window: Option<(f64, f64)>) -> AudioTrack {
    AudioTrack {
        order,
        ..fixture_track(window)
    }
}
