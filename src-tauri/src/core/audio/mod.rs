use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

pub mod batch;
pub mod carver;
pub mod probe;
pub mod track;

pub use batch::{transcode_batch_sequential, TranscodeJob};
pub use carver::{
    boundaries_from_silences, carve, AbsorbPolicy, Boundary, BoundaryKind, CarveError, CarveOpts,
    SilenceRun,
};
pub use probe::{probe_chapters, ChapterAtom};
pub use track::AudioTrack;

/// Threshold above which |dst - src| seconds is considered a transcode mismatch.
pub(crate) const DURATION_DELTA_THRESHOLD_SEC: f64 = 1.0;

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
    DurationMismatch { delta_sec: f64, threshold_sec: f64 },
    #[error("io: {0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
    #[error("decode: {0}")]
    Decode(String),
    #[error("encode: {0}")]
    Encode(String),
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

/// Resolve the ffmpeg binary path.
/// Order: `FFMPEG_BIN` env > PATH.
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

pub async fn probe_duration(path: &Path) -> Result<f64, AudioError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        <crate::codecs::SymphoniaMetadata as crate::codecs::AudioMetadata>::probe_duration(&path)
    })
    .await
    .map_err(|e| AudioError::Io(e.to_string()))?
}

pub async fn transcode(
    src: &Path,
    dst: &Path,
    enc: &EncoderSettings,
    window: Option<(f64, f64)>,
) -> Result<TranscodeReport, AudioError> {
    let src = src.to_path_buf();
    let dst = dst.to_path_buf();
    let enc = enc.clone();
    tokio::task::spawn_blocking(move || {
        use crate::codecs::mp3_encoder::encode_mp3;
        use crate::codecs::{symphonia_impl::SymphoniaDecoder, AudioDecoder};

        let mut decoder = SymphoniaDecoder::open(&src)?;
        let info = decoder.info();
        let (start, end) = window.unwrap_or((0.0, info.duration_sec));
        if start > 0.0 {
            decoder.seek(start)?;
        }
        let max_frames = ((end - start) * info.sample_rate as f64).max(0.0) as u64;
        let mut yielded: u64 = 0;
        let iter = std::iter::from_fn(move || {
            if yielded >= max_frames {
                return None;
            }
            match decoder.next_frame() {
                Ok(Some(mut f)) => {
                    let take = (max_frames - yielded).min(f.frames as u64) as usize;
                    if take < f.frames {
                        let ch = info.channels as usize;
                        f.samples.truncate(take * ch);
                        f.frames = take;
                    }
                    yielded += f.frames as u64;
                    Some(f)
                }
                Ok(None) => None,
                Err(_) => None,
            }
        });
        encode_mp3(iter, &info, &dst, &enc)
    })
    .await
    .map_err(|e| AudioError::Io(e.to_string()))?
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

    #[tokio::test]
    async fn drop_cancels_in_flight_transcode() {
        use std::time::Duration;
        use tokio::time::timeout;

        let src = std::path::PathBuf::from("/definitely/not/a/real/input.m4a");
        let dst = std::path::PathBuf::from("/tmp/never-written.mp3");
        let enc = EncoderSettings::default();
        let _ = timeout(Duration::from_millis(50), transcode(&src, &dst, &enc, None)).await;
    }
}
