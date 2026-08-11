use lingq_upload_lib::codecs::SymphoniaDecoder;
use lingq_upload_lib::core::audio::AudioError;
use lingq_upload_lib::transcribe::extract_sample;
use lingq_upload_lib::transcribe::sample::{SampleSide, SampleWindow};
use lingq_upload_lib::AudioDecoder;
use tokio_util::sync::CancellationToken;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio")
        .join(name)
}

#[tokio::test]
async fn extraction_uses_a_new_owned_temp_path_and_whisper_codec_settings() {
    let window = SampleWindow {
        side: SampleSide::Head,
        attempt: 0,
        track_index: 0,
        path: fixture("probe_3min.mp3"),
        start_sec: 30.0,
        end_sec: 60.0,
    };

    let extracted = extract_sample(&window, &CancellationToken::new())
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
