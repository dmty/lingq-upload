use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AudioTrack {
    pub order: usize,
    pub path: PathBuf,
    pub duration_sec: Option<f64>,
    pub title: Option<String>,
}
