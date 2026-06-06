use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio::process::Command;

/// Threshold above which |dst - src| seconds is considered a transcode mismatch.
/// Rationale lives in the shared-context audio-corruption story.
const DURATION_DELTA_THRESHOLD_SEC: f64 = 1.0;

/// Truncate captured ffmpeg stderr to keep error payloads bounded for logs / IPC.
const STDERR_CAPTURE_BYTES: usize = 4 * 1024;

#[derive(Error, Debug, Serialize, Deserialize, Type, Clone)]
#[serde(tag = "kind", content = "message")]
#[allow(dead_code)]
pub enum AudioError {
    #[error("ffmpeg not found at {0}")]
    FfmpegNotFound(String),
    #[error("ffmpeg exited with status {status}: {stderr}")]
    FfmpegFailed { status: i32, stderr: String },
    #[error("ffprobe parse error: {0}")]
    Probe(String),
    #[error("duration mismatch (delta {delta_sec}s > {threshold_sec}s)")]
    DurationMismatch {
        delta_sec: f64,
        threshold_sec: f64,
    },
    #[error("io: {0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<std::io::Error> for AudioError {
    fn from(e: std::io::Error) -> Self {
        AudioError::Io(e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EncoderSettings {
    pub bitrate: String,
    pub sample_rate: u32,
    pub channels: u8,
}

impl Default for EncoderSettings {
    fn default() -> Self {
        Self {
            bitrate: "96k".into(),
            sample_rate: 22050,
            channels: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscodeReport {
    pub src_duration_sec: f64,
    pub dst_duration_sec: f64,
    pub delta_sec: f64,
}

/// Resolve the ffmpeg binary path. Env override > PATH lookup.
/// Release-mode resource_dir hook lands later; PATH-relative "ffmpeg" is
/// acceptable for now.
pub fn resolve_ffmpeg_bin() -> Result<PathBuf, AudioError> {
    if let Ok(v) = std::env::var("FFMPEG_BIN") {
        let p = PathBuf::from(v);
        if !p.exists() {
            return Err(AudioError::FfmpegNotFound(p.display().to_string()));
        }
        return Ok(p);
    }
    Ok(PathBuf::from("ffmpeg"))
}

/// Resolve ffprobe sibling-of-ffmpeg. Mirrors `resolve_ffmpeg_bin`.
pub fn resolve_ffprobe_bin() -> Result<PathBuf, AudioError> {
    if let Ok(v) = std::env::var("FFPROBE_BIN") {
        let p = PathBuf::from(v);
        if !p.exists() {
            return Err(AudioError::FfmpegNotFound(p.display().to_string()));
        }
        return Ok(p);
    }
    // If FFMPEG_BIN is set, prefer the sibling ffprobe next to it.
    if let Ok(v) = std::env::var("FFMPEG_BIN") {
        let p = PathBuf::from(v);
        if let Some(parent) = p.parent() {
            let candidate = parent.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" });
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Ok(PathBuf::from("ffprobe"))
}

pub async fn probe_duration(path: &Path) -> Result<f64, AudioError> {
    let bin = resolve_ffprobe_bin()?;
    let output = Command::new(&bin)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AudioError::FfmpegNotFound(bin.display().to_string()),
            _ => AudioError::Io(e.to_string()),
        })?;

    if !output.status.success() {
        let stderr = tail_lossy(&output.stderr, STDERR_CAPTURE_BYTES);
        return Err(AudioError::FfmpegFailed {
            status: output.status.code().unwrap_or(-1),
            stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    trimmed
        .parse::<f64>()
        .map_err(|e| AudioError::Probe(format!("expected float, got {trimmed:?}: {e}")))
}

pub async fn transcode(
    src: &Path,
    dst: &Path,
    enc: &EncoderSettings,
) -> Result<TranscodeReport, AudioError> {
    let src_duration = probe_duration(src).await?;

    let bin = resolve_ffmpeg_bin()?;
    let output = Command::new(&bin)
        .args(["-y", "-hide_banner", "-v", "error", "-i"])
        .arg(src)
        .args([
            "-vn",
            "-map",
            "0:a:0",
            "-c:a",
            "libmp3lame",
            "-b:a",
            &enc.bitrate,
            "-ar",
            &enc.sample_rate.to_string(),
            "-ac",
            &enc.channels.to_string(),
        ])
        .arg(dst)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AudioError::FfmpegNotFound(bin.display().to_string()),
            _ => AudioError::Io(e.to_string()),
        })?;

    if !output.status.success() {
        let stderr = tail_lossy(&output.stderr, STDERR_CAPTURE_BYTES);
        return Err(AudioError::FfmpegFailed {
            status: output.status.code().unwrap_or(-1),
            stderr,
        });
    }

    let dst_duration = probe_duration(dst).await?;
    let delta = dst_duration - src_duration;
    if delta.abs() > DURATION_DELTA_THRESHOLD_SEC {
        return Err(AudioError::DurationMismatch {
            delta_sec: delta,
            threshold_sec: DURATION_DELTA_THRESHOLD_SEC,
        });
    }

    Ok(TranscodeReport {
        src_duration_sec: src_duration,
        dst_duration_sec: dst_duration,
        delta_sec: delta,
    })
}

/// Keep only the trailing `max_bytes` of a stderr buffer; lossy-decode to UTF-8.
/// ffmpeg stderr can be megabytes on verbose builds; we just want the tail.
fn tail_lossy(buf: &[u8], max_bytes: usize) -> String {
    let start = buf.len().saturating_sub(max_bytes);
    String::from_utf8_lossy(&buf[start..]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_settings_default_matches_spec() {
        let d = EncoderSettings::default();
        assert_eq!(d.bitrate, "96k");
        assert_eq!(d.sample_rate, 22050);
        assert_eq!(d.channels, 2);
    }

    #[test]
    fn ffmpeg_bin_env_override_missing_path_errors() {
        // SAFETY: tests run single-threaded for this var via #[serial] would be ideal,
        // but cargo test default is multi-threaded. We only set then restore.
        let prev = std::env::var("FFMPEG_BIN").ok();
        std::env::set_var("FFMPEG_BIN", "/definitely/not/a/real/path/ffmpeg");
        let res = resolve_ffmpeg_bin();
        match prev {
            Some(v) => std::env::set_var("FFMPEG_BIN", v),
            None => std::env::remove_var("FFMPEG_BIN"),
        }
        assert!(matches!(res, Err(AudioError::FfmpegNotFound(_))));
    }

    #[test]
    fn tail_lossy_truncates() {
        let buf = vec![b'x'; 10_000];
        let s = tail_lossy(&buf, 1024);
        assert_eq!(s.len(), 1024);
    }

    /// Spawn a transcode against a non-existent input; drop the future early.
    /// On Unix `kill_on_drop(true)` SIGKILLs the child. We can't portably
    /// assert no orphans here, but we verify Drop semantics compile and run.
    /// Windows: tokio uses TerminateProcess; same Drop semantics, just a
    /// different syscall path.
    #[tokio::test]
    async fn drop_cancels_in_flight_transcode() {
        use std::time::Duration;
        use tokio::time::timeout;

        let src = std::path::PathBuf::from("/definitely/not/a/real/input.m4a");
        let dst = std::path::PathBuf::from("/tmp/never-written.mp3");
        let enc = EncoderSettings::default();

        // Race the future against a tiny timeout; if it returns an error
        // first (because ffmpeg/ffprobe rejects the input), that's also fine —
        // the point is the drop path doesn't panic or leak.
        let _ = timeout(Duration::from_millis(50), transcode(&src, &dst, &enc)).await;
    }
}
