use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AudioTrack {
    pub order: usize,
    pub path: PathBuf,
    pub duration_sec: Option<f64>,
    pub title: Option<String>,
    /// None means the whole file; Some((start, end)) names a slice in seconds
    /// extracted from an embedded chapter atom. See AD-023 and
    /// `docs/specs/m4b-chapters.md`.
    #[serde(default)]
    pub window: Option<(f64, f64)>,
}
