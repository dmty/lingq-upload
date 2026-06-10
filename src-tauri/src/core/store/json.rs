use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::{canonicalise_selection, safe_path_segment, ProjectStore, StoreError};
use crate::core::epub::ChapterId;
use crate::core::identity::ProjectId;
use crate::core::matcher::{apply_mapping_op as apply_pure, MappingOp, MappingState};
use crate::core::project::{ChapterReceipt, Project, ProjectSummary};

pub struct JsonProjectStore {
    root: PathBuf,
    /// Per-project write lock. Acquired around read-modify-write paths
    /// (`patch_chapter`, `set_selection`) so two concurrent edits to the
    /// same project cannot interleave their loads and lose the older one.
    write_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
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
        Self {
            root: root.into(),
            write_locks: Mutex::new(HashMap::new()),
        }
    }

    /// Hand out (cloning) the `Arc<Mutex<()>>` for `id`, creating it on
    /// first touch. Callers hold the inner mutex for the entire RMW window.
    fn write_lock(&self, id: &ProjectId) -> Arc<Mutex<()>> {
        let key = id.join_key();
        let mut map = self
            .write_locks
            .lock()
            .expect("write-locks registry poisoned");
        map.entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
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

/// Atomic write: write to a unique `path.tmp.*`, fsync, rename over `path`,
/// fsync the parent dir. Power-cut between write and rename leaves the prior
/// file intact (D1 AC2). The pid + counter suffix keeps concurrent writers
/// (other processes; in-process callers race only up to the per-project lock)
/// from sharing one tmp file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path.parent().ok_or_else(|| StoreError::Io {
        path: path.to_path_buf(),
        message: "path has no parent".into(),
    })?;
    fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    let tmp = path.with_extension(format!(
        "json.tmp.{}.{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    {
        let mut f = fs::File::create(&tmp).map_err(|e| io_err(&tmp, e))?;
        f.write_all(bytes).map_err(|e| io_err(&tmp, e))?;
        f.sync_all().map_err(|e| io_err(&tmp, e))?;
    }
    fs::rename(&tmp, path).map_err(|e| io_err(path, e))?;
    // Durability of the rename itself: fsync the directory entry. Windows
    // cannot open directories as files, so unix-only.
    #[cfg(unix)]
    {
        let dir = fs::File::open(parent).map_err(|e| io_err(parent, e))?;
        dir.sync_all().map_err(|e| io_err(parent, e))?;
    }
    Ok(())
}

fn serialise_project(p: &Project, path: &Path) -> Result<Vec<u8>, StoreError> {
    serde_json::to_vec_pretty(p).map_err(|e| StoreError::Corrupt {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

impl ProjectStore for JsonProjectStore {
    fn put(&self, p: &Project) -> Result<(), StoreError> {
        let lock = self.write_lock(&p.id);
        let _guard = lock.lock().expect("project write lock poisoned");
        let path = self.project_path(&p.id);
        let bytes = serialise_project(p, &path)?;
        write_atomic(&path, &bytes)
    }

    fn update(
        &self,
        id: &ProjectId,
        f: &mut dyn FnMut(&mut Project),
    ) -> Result<Project, StoreError> {
        let lock = self.write_lock(id);
        let _guard = lock.lock().expect("project write lock poisoned");
        let mut project = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound { key: id.join_key() })?;
        f(&mut project);
        let path = self.project_path(&project.id);
        let bytes = serialise_project(&project, &path)?;
        write_atomic(&path, &bytes)?;
        Ok(project)
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

    fn patch_chapter(
        &self,
        id: &ProjectId,
        index: usize,
        receipt: ChapterReceipt,
    ) -> Result<(), StoreError> {
        let lock = self.write_lock(id);
        let _guard = lock.lock().expect("project write lock poisoned");
        let mut project = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound { key: id.join_key() })?;
        let len = project.receipts.len();
        if index >= len {
            return Err(StoreError::OutOfBounds { index, len });
        }
        project.receipts[index] = receipt;
        let path = self.project_path(&project.id);
        let bytes = serialise_project(&project, &path)?;
        write_atomic(&path, &bytes)
    }

    fn set_selection(
        &self,
        id: &ProjectId,
        skipped_ids: &[ChapterId],
    ) -> Result<(), StoreError> {
        let lock = self.write_lock(id);
        let _guard = lock.lock().expect("project write lock poisoned");
        let mut project = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound { key: id.join_key() })?;
        project.skipped_chapters = canonicalise_selection(skipped_ids);
        let path = self.project_path(&project.id);
        let bytes = serialise_project(&project, &path)?;
        write_atomic(&path, &bytes)
    }

    fn apply_mapping_op(
        &self,
        id: &ProjectId,
        op: MappingOp,
        expected_op_id: u64,
    ) -> Result<MappingState, StoreError> {
        let lock = self.write_lock(id);
        let _guard = lock.lock().expect("project write lock poisoned");
        let mut project = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound { key: id.join_key() })?;
        let current = project.mapping.clone().unwrap_or_default();
        if expected_op_id != current.op_id + 1 {
            return Err(StoreError::MappingStaleOp {
                server: current.op_id,
                expected: expected_op_id,
            });
        }
        let next = apply_pure(&current, op).map_err(StoreError::Mapping)?;
        project.mapping = Some(next.clone());
        let path = self.project_path(&project.id);
        let bytes = serialise_project(&project, &path)?;
        write_atomic(&path, &bytes)?;
        Ok(next)
    }
}
