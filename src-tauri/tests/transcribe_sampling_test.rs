use lingq_upload_lib::codecs::SymphoniaDecoder;
use lingq_upload_lib::core::audio::AudioError;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::ingest::AudioSource;
use lingq_upload_lib::transcribe::sample::{SampleSide, SampleWindow};
use lingq_upload_lib::transcribe::{
    extract_sample, resolve_and_plan_sample_windows, AlignmentConfig,
};
use lingq_upload_lib::AudioDecoder;
use tokio_util::sync::CancellationToken;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio")
        .join(name)
}

#[tokio::test]
async fn sampling_orchestration_probes_real_boundaries_before_extracting() {
    let inputs = tempfile::tempdir().expect("input tempdir");
    let head_path = inputs.path().join("head.m4b");
    let tail_path = inputs.path().join("tail.m4b");
    std::fs::copy(fixture("synth_chapters_narrative.m4b"), &head_path).expect("copy head fixture");
    std::fs::copy(fixture("synth_chapters_narrative.m4b"), &tail_path).expect("copy tail fixture");
    let mut project = Project::new_test(ProjectId::from_title_author("samples", "test"), "samples");
    project.sources.audio = Some(AudioSource::MultipleFiles(vec![
        head_path.clone(),
        tail_path.clone(),
    ]));

    let plan = resolve_and_plan_sample_windows(&project, &AlignmentConfig::default())
        .await
        .expect("resolve tracks, probe boundary files, and plan samples");

    assert_eq!(plan.head.initial.path, head_path);
    assert_eq!(plan.tail.initial.path, tail_path);
    assert!(plan.head.initial.end_sec - plan.head.initial.start_sec >= 10.0);
    assert!(plan.tail.initial.end_sec - plan.tail.initial.start_sec >= 10.0);

    let extracted = extract_sample(&plan.head.initial, &CancellationToken::new())
        .await
        .expect("extract sample");

    assert_eq!(
        extracted.path().file_name().and_then(|name| name.to_str()),
        Some("head-0.mp3")
    );
    assert!(extracted.path().exists());
    assert!(extracted.report().delta_sec.abs() <= 1.0);
    let decoder = SymphoniaDecoder::open(extracted.path()).expect("open extracted sample");
    assert_eq!(decoder.info().channels, 1);
    assert_eq!(decoder.info().sample_rate, 16_000);
    let output = extracted.path().to_owned();
    drop(extracted);
    assert!(!output.exists());
}

#[tokio::test]
async fn pre_cancelled_extraction_creates_no_result() {
    let window = SampleWindow {
        side: SampleSide::Tail,
        attempt: 1,
        track_index: 0,
        path: fixture("probe_3min.mp3"),
        start_sec: 90.0,
        end_sec: 120.0,
    };
    let cancel = CancellationToken::new();
    cancel.cancel();

    let result = extract_sample(&window, &cancel).await;

    assert!(matches!(result, Err(AudioError::Cancelled)));
}

#[tokio::test]
async fn cancellation_during_blocking_extraction_suppresses_its_result() {
    let window = SampleWindow {
        side: SampleSide::Head,
        attempt: 0,
        track_index: 0,
        path: fixture("probe_3min.mp3"),
        start_sec: 30.0,
        end_sec: 60.0,
    };
    let cancel = CancellationToken::new();
    let cancel_during_decode = cancel.clone();
    let cancellation = tokio::spawn(async move {
        tokio::task::yield_now().await;
        cancel_during_decode.cancel();
    });

    let result = extract_sample(&window, &cancel).await;
    cancellation.await.expect("cancellation task");

    assert!(matches!(result, Err(AudioError::Cancelled)));
}
