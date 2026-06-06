use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{safe_path_segment, ProjectStore, StoreError};
use crate::core::identity::ProjectId;
use crate::core::project::{Project, ProjectSummary};

pub struct JsonProjectStore {
    root: PathBuf,
}

impl JsonProjectStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn dir_for(&self, id: &ProjectId) -> PathBuf {
        // `join_key()` contains `:`, which is illegal in Windows path
        // segments. Sanitize before touching the filesystem.
        self.root
            .join("projects")
            .join(safe_path_segment(&id.join_key()))
    }

    fn project_path(&self, id: &ProjectId) -> PathBuf {
        self.dir_for(id).join("project.json")
    }
}

fn io_err(path: &Path, e: std::io::Error) -> StoreError {
    StoreError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    }
}

/// Atomic write: write to `path.tmp`, fsync, rename over `path`.
/// Power-cut between write and rename leaves the prior file intact (D1 AC2).
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().ok_or_else(|| StoreError::Io {
        path: path.to_path_buf(),
        message: "path has no parent".into(),
    })?;
    fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| io_err(&tmp, e))?;
        f.write_all(bytes).map_err(|e| io_err(&tmp, e))?;
        f.sync_all().map_err(|e| io_err(&tmp, e))?;
    }
    fs::rename(&tmp, path).map_err(|e| io_err(path, e))?;
    Ok(())
}

impl ProjectStore for JsonProjectStore {
    fn put(&self, p: &Project) -> Result<(), StoreError> {
        let path = self.project_path(&p.id);
        let bytes = serde_json::to_vec_pretty(p).map_err(|e| StoreError::Corrupt {
            path: path.clone(),
            message: e.to_string(),
        })?;
        write_atomic(&path, &bytes)
    }

    fn get(&self, id: &ProjectId) -> Result<Option<Project>, StoreError> {
        let path = self.project_path(id);
        match fs::read(&path) {
            Ok(bytes) => {
                let p: Project =
                    serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt {
                        path: path.clone(),
                        message: e.to_string(),
                    })?;
                Ok(Some(p))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err(&path, e)),
        }
    }

    fn list(&self) -> Result<Vec<ProjectSummary>, StoreError> {
        let projects = self.root.join("projects");
        if !projects.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let entries = fs::read_dir(&projects).map_err(|e| io_err(&projects, e))?;
        for ent in entries {
            let ent = ent.map_err(|e| io_err(&projects, e))?;
            let pj = ent.path().join("project.json");
            if !pj.is_file() {
                continue;
            }
            let bytes = match fs::read(&pj) {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    tracing::warn!(path = %pj.display(), error = %e, "skipping unreadable project.json");
                    continue;
                }
            };
            match serde_json::from_slice::<Project>(&bytes) {
                Ok(p) => out.push((&p).into()),
                Err(e) => {
                    tracing::warn!(path = %pj.display(), error = %e, "skipping corrupt project.json");
                    continue;
                }
            }
        }
        out.sort_by(|a: &ProjectSummary, b| a.title.cmp(&b.title));
        Ok(out)
    }
}
