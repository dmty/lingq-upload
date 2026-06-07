use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{safe_path_segment, ProjectStore, StoreError};
use crate::core::identity::ProjectId;
use crate::core::project::{Project, ProjectSummary};

pub struct JsonProjectStore {
    root: PathBuf,
}

/// Diagnostics emitted by [`JsonProjectStore::health`].
///
/// Additive sibling of [`ProjectStore::list`] that surfaces counts the trait
/// API intentionally suppresses (corrupt files, deduped duplicates). The
/// trait stays compatible across stores; callers that need to alert the user
/// on data loss reach for `health`.
#[derive(Debug, Default, Clone)]
pub struct ListHealth {
    pub ok: usize,
    pub corrupt: Vec<PathBuf>,
    pub deduped: Vec<PathBuf>,
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

    /// Walk `projects/` and classify every `project.json` as ok, corrupt, or
    /// deduped. Deterministic across platforms: the dedupe winner is the file
    /// with the most recent mtime per `join_key` (path tiebreak when mtimes
    /// collide), so two runs on the same on-disk state always pick the same
    /// survivor.
    pub fn health(&self) -> Result<ListHealth, StoreError> {
        let mut health = ListHealth::default();
        let scan = match self.scan()? {
            Some(s) => s,
            None => return Ok(health),
        };
        health.corrupt = scan.corrupt;

        let mut entries = scan.entries;
        sort_for_dedup(&mut entries);

        let mut last_key: Option<String> = None;
        for entry in entries {
            if last_key.as_ref() == Some(&entry.key) {
                tracing::warn!(
                    path = %entry.path.display(),
                    id = %entry.key,
                    "skipping duplicate project.json with already-seen id",
                );
                health.deduped.push(entry.path);
            } else {
                last_key = Some(entry.key.clone());
                health.ok += 1;
            }
        }
        Ok(health)
    }

    fn scan(&self) -> Result<Option<ScanResult>, StoreError> {
        let projects = self.root.join("projects");
        if !projects.exists() {
            return Ok(None);
        }
        let mut result = ScanResult::default();
        let dir_entries = fs::read_dir(&projects).map_err(|e| io_err(&projects, e))?;
        for ent in dir_entries {
            let ent = ent.map_err(|e| io_err(&projects, e))?;
            // Soft-deleted projects live under projects/.trash/. Skip the whole
            // subtree; trash commands read it via the dedicated module.
            if ent.file_name() == crate::core::library::trash::TRASH_DIRNAME {
                continue;
            }
            let pj = ent.path().join("project.json");
            if !pj.is_file() {
                continue;
            }
            let bytes = match fs::read(&pj) {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    tracing::warn!(path = %pj.display(), error = %e, "skipping unreadable project.json");
                    result.corrupt.push(pj);
                    continue;
                }
            };
            let project: Project = match serde_json::from_slice(&bytes) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(path = %pj.display(), error = %e, "skipping corrupt project.json");
                    result.corrupt.push(pj);
                    continue;
                }
            };
            let mtime = fs::metadata(&pj)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let key = project.id.join_key();
            result.entries.push(ScanEntry {
                key,
                mtime,
                path: pj,
                project,
            });
        }
        Ok(Some(result))
    }
}

#[derive(Default)]
struct ScanResult {
    entries: Vec<ScanEntry>,
    corrupt: Vec<PathBuf>,
}

struct ScanEntry {
    key: String,
    mtime: SystemTime,
    path: PathBuf,
    project: Project,
}

// (join_key ASC, mtime DESC, path ASC). Freshest survives per dup group;
// path tiebreak guarantees a deterministic survivor across filesystems whose
// mtime resolution rounds to the second.
fn sort_for_dedup(entries: &mut [ScanEntry]) {
    entries.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| b.mtime.cmp(&a.mtime))
            .then_with(|| a.path.cmp(&b.path))
    });
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
        let scan = match self.scan()? {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };

        let mut entries = scan.entries;
        sort_for_dedup(&mut entries);

        let mut out: Vec<ProjectSummary> = Vec::with_capacity(entries.len());
        let mut last_key: Option<String> = None;
        for entry in entries {
            if last_key.as_ref() == Some(&entry.key) {
                tracing::warn!(
                    path = %entry.path.display(),
                    id = %entry.key,
                    "skipping duplicate project.json with already-seen id",
                );
                continue;
            }
            last_key = Some(entry.key.clone());
            out.push((&entry.project).into());
        }

        out.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(out)
    }
}
