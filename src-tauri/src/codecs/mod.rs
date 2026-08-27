use std::path::Path;

use crate::core::audio::{AudioError, ChapterAtom};

pub mod pcm;
pub use pcm::{PcmFrame, StreamInfo};

pub mod symphonia_impl;
pub use symphonia_impl::{SymphoniaDecoder, SymphoniaMetadata};

pub mod mp3_encoder;
pub mod mp4_chapters;
pub mod silence;
pub use silence::detect_silence;

pub trait AudioDecoder: Send {
    fn open(path: &Path) -> Result<Self, AudioError>
    where
        Self: Sized;
    fn info(&self) -> StreamInfo;
    fn seek(&mut self, sec: f64) -> Result<(), AudioError>;
    fn next_frame(&mut self) -> Result<Option<PcmFrame>, AudioError>;
}

/// Cover art carried inside an audio container. `.m4b` / `.m4a` / `.mp4`
/// store it in the `covr` atom, `.mp3` in an ID3v2 `APIC` frame, `.flac` in a
/// `PICTURE` block and Ogg in `METADATA_BLOCK_PICTURE`; `.wav` has no standard
/// place for one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedCover {
    pub data: Vec<u8>,
    /// The container's own media-type string, e.g. `image/jpeg`. Containers
    /// are inconsistent here, so treat it as a hint rather than a guarantee.
    pub media_type: String,
}

pub trait AudioMetadata: Send {
    fn probe_chapters(path: &Path) -> Result<Vec<ChapterAtom>, AudioError>
    where
        Self: Sized;
    fn probe_duration(path: &Path) -> Result<f64, AudioError>
    where
        Self: Sized;
    /// The file's front cover, when it has one. `Ok(None)` covers both "no
    /// art" and "format that cannot carry art".
    fn probe_cover(path: &Path) -> Result<Option<EmbeddedCover>, AudioError>
    where
        Self: Sized;
}
