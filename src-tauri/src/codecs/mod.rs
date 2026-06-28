use std::path::Path;

use crate::core::audio::{AudioError, ChapterAtom};

pub mod pcm;
pub use pcm::{PcmFrame, StreamInfo};

pub trait AudioDecoder: Send {
    fn open(path: &Path) -> Result<Self, AudioError>
    where
        Self: Sized;
    fn info(&self) -> StreamInfo;
    fn seek(&mut self, sec: f64) -> Result<(), AudioError>;
    fn next_frame(&mut self) -> Result<Option<PcmFrame>, AudioError>;
}

pub trait AudioMetadata: Send {
    fn probe_chapters(path: &Path) -> Result<Vec<ChapterAtom>, AudioError>
    where
        Self: Sized;
    fn probe_duration(path: &Path) -> Result<f64, AudioError>
    where
        Self: Sized;
}
