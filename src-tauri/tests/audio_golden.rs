//! Audio transcode golden test.
//!
//! Writes a silent WAV fixture via hound, transcodes it through
//! `core::audio::transcode`, then probes the output via `SymphoniaDecoder`
//! and asserts the shape matches `silence.golden.json` within tolerance.

mod support;

use std::path::PathBuf;

use serde::Deserialize;

use lingq_upload_lib::codecs::symphonia_impl::SymphoniaDecoder;
use lingq_upload_lib::codecs::AudioDecoder;
use lingq_upload_lib::core::audio;

#[derive(Deserialize)]
struct Golden {
    codec_name: String,
    sample_rate: u32,
    channels: u32,
    duration_sec_target: f64,
    duration_sec_tolerance: f64,
}

#[tokio::test]
async fn silence_transcode_matches_golden() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_src = manifest_dir.join("tests/fixtures/audio/silence.wav");
    let golden_path = manifest_dir.join("tests/fixtures/audio/silence.golden.json");

    support::mk_fixture::write_silence_m4a_like(&fixture_src, 5);

    let golden_raw = std::fs::read_to_string(&golden_path).expect("read golden");
    let golden: Golden = serde_json::from_str(&golden_raw).expect("parse golden");

    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("silence.mp3");
    let enc = audio::EncoderSettings::default();

    let report = audio::transcode(&fixture_src, &dst, &enc, None)
        .await
        .expect("transcode");

    assert!(
        report.delta_sec.abs() < 0.1,
        "delta {} exceeded 0.1s",
        report.delta_sec
    );

    let info = SymphoniaDecoder::open(&dst).expect("probe mp3").info();
    assert_eq!(info.codec, golden.codec_name, "codec_name");
    assert_eq!(info.sample_rate, golden.sample_rate, "sample_rate");
    assert_eq!(info.channels as u32, golden.channels, "channels");

    // bit_rate tolerance — symphonia doesn't expose bit_rate so we skip that check.
    // ponytail: bit_rate check removed; symphonia StreamInfo has no bit_rate field.
    let duration = info.duration_sec;
    let dur_delta = (duration - golden.duration_sec_target).abs();
    assert!(
        dur_delta <= golden.duration_sec_tolerance,
        "duration {} outside ±{} of {}",
        duration,
        golden.duration_sec_tolerance,
        golden.duration_sec_target
    );
}

#[tokio::test]
async fn m4b_extension_is_accepted() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_src = manifest_dir.join("tests/fixtures/audio/silence.wav");
    support::mk_fixture::write_silence_m4a_like(&fixture_src, 5);

    let tmp = tempfile::tempdir().expect("tempdir");
    let renamed = tmp.path().join("silence.m4b");
    std::fs::copy(&fixture_src, &renamed).expect("copy to .m4b");

    let dst = tmp.path().join("out.mp3");
    let enc = audio::EncoderSettings::default();
    audio::transcode(&renamed, &dst, &enc, None)
        .await
        .expect("m4b transcode");
    assert!(dst.exists());
}
