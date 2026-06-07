//! Audio transcode golden test.
//!
//! Generates `tests/fixtures/audio/silence.m4a` at runtime via the `ffmpeg`
//! CLI (deterministic, ~30KB — not worth checking into git), transcodes it
//! through `core::audio::transcode`, then ffprobes the output and asserts the
//! shape matches `silence.golden.json` within tolerance.
//!
//! Skipped (early-return) if `ffmpeg`/`ffprobe` are missing from PATH so devs
//! without ffmpeg installed don't see red. CI installs ffmpeg per OS.

use std::path::{Path, PathBuf};
use std::process::Command as SyncCommand;

use serde::Deserialize;

use lingq_upload_lib::core::audio;

#[derive(Deserialize)]
struct Golden {
    codec_name: String,
    sample_rate: u32,
    channels: u32,
    bit_rate_target: u64,
    bit_rate_tolerance_pct: f64,
    duration_sec_target: f64,
    duration_sec_tolerance: f64,
}

#[derive(Deserialize)]
struct FfprobeOut {
    streams: Vec<FfprobeStream>,
    format: FfprobeFormat,
}

#[derive(Deserialize)]
struct FfprobeStream {
    codec_name: String,
    sample_rate: String,
    channels: u32,
    bit_rate: Option<String>,
}

#[derive(Deserialize)]
struct FfprobeFormat {
    duration: String,
    bit_rate: Option<String>,
}

fn which(bin: &str) -> Option<PathBuf> {
    SyncCommand::new("which")
        .arg(bin)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
}

fn ffmpeg_available() -> bool {
    which("ffmpeg").is_some() && which("ffprobe").is_some()
}

fn ensure_silence_fixture(path: &Path) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixtures dir");
    }
    let status = SyncCommand::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo",
            "-t",
            "5",
            "-c:a",
            "aac",
        ])
        .arg(path)
        .status()
        .expect("spawn ffmpeg to create silence fixture");
    assert!(status.success(), "ffmpeg silence-fixture generation failed");
}

fn probe_full(path: &Path) -> FfprobeOut {
    let out = SyncCommand::new("ffprobe")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .expect("spawn ffprobe");
    assert!(
        out.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("parse ffprobe json")
}

#[tokio::test]
async fn silence_transcode_matches_golden() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping audio_golden");
        return;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_src = manifest_dir.join("tests/fixtures/audio/silence.m4a");
    let golden_path = manifest_dir.join("tests/fixtures/audio/silence.golden.json");

    ensure_silence_fixture(&fixture_src);

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

    let probed = probe_full(&dst);
    let stream = probed
        .streams
        .iter()
        .find(|s| s.codec_name == golden.codec_name)
        .expect("matching audio stream");

    assert_eq!(stream.codec_name, golden.codec_name, "codec_name");
    assert_eq!(
        stream.sample_rate.parse::<u32>().expect("sample_rate u32"),
        golden.sample_rate,
        "sample_rate"
    );
    assert_eq!(stream.channels, golden.channels as u32, "channels");

    // bit_rate may live on stream or format depending on container; pick whichever's present.
    let bit_rate: u64 = stream
        .bit_rate
        .as_deref()
        .or(probed.format.bit_rate.as_deref())
        .and_then(|s| s.parse().ok())
        .expect("bit_rate");
    let tol_abs = (golden.bit_rate_target as f64) * (golden.bit_rate_tolerance_pct / 100.0);
    let bit_rate_delta = (bit_rate as f64 - golden.bit_rate_target as f64).abs();
    assert!(
        bit_rate_delta <= tol_abs,
        "bit_rate {} outside ±{}% of {} (delta {})",
        bit_rate,
        golden.bit_rate_tolerance_pct,
        golden.bit_rate_target,
        bit_rate_delta
    );

    let duration: f64 = probed.format.duration.parse().expect("duration f64");
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
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not on PATH — skipping m4b_extension_is_accepted");
        return;
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_src = manifest_dir.join("tests/fixtures/audio/silence.m4a");
    ensure_silence_fixture(&fixture_src);

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
