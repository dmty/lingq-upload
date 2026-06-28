//! Round-trip cover for `cmd_set_cover`: the command is a thin wrapper over
//! `ProjectStore::update` (like `cmd_set_absorb_policy`), so this exercises
//! that closure path and confirms `cover_path` survives a process boundary.

use std::path::PathBuf;
use std::sync::Arc;

use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::core::store::{JsonProjectStore, ProjectStore};
use tempfile::TempDir;

#[test]
fn set_cover_persists_across_reopen() {
    let store_dir = TempDir::new().unwrap();
    let store: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(store_dir.path()));

    let id = ProjectId::from_title_author("Cover Book", "Author");
    let project = Project::new_test(id.clone(), "Cover Book");
    store.put(&project).unwrap();

    // Starts with no cover.
    assert!(store.get(&id).unwrap().unwrap().cover_path.is_none());

    // Apply the same mutation `cmd_set_cover` performs.
    let cover = PathBuf::from("/covers/botchan.jpg");
    store
        .update(&id, &mut |p| p.cover_path = Some(cover.clone()))
        .expect("update ok");

    // Drop the handle, reopen the store from disk, confirm it survived.
    drop(store);
    let reopened: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(store_dir.path()));
    let after = reopened.get(&id).unwrap().unwrap();
    assert_eq!(after.cover_path, Some(PathBuf::from("/covers/botchan.jpg")));
}
