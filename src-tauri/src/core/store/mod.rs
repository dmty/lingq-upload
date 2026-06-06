pub mod json;
pub mod memory;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::core::identity::ProjectId;
use crate::core::project::{Project, ProjectSummary};

pub use json::JsonProjectStore;
pub use memory::InMemoryProjectStore;

#[derive(Debug, Error, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "message")]
pub enum StoreError {
    #[error("io error at {path}: {message}")]
    Io { path: PathBuf, message: String },
    #[error("corrupt JSON at {path}: {message}")]
    Corrupt { path: PathBuf, message: String },
    #[error("not found: {key}")]
    NotFound { key: String },
}

pub trait ProjectStore: Send + Sync {
    fn put(&self, p: &Project) -> Result<(), StoreError>;
    fn get(&self, id: &ProjectId) -> Result<Option<Project>, StoreError>;
    fn list(&self) -> Result<Vec<ProjectSummary>, StoreError>;
}
