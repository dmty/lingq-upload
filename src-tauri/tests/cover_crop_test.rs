//! `cmd_set_cover_bytes` — the crop editor's save path. It writes
//! already-encoded bytes over `cover.{ext}` and keeps the pre-crop image as
//! `cover-original.{ext}` so later crops re-cut the full picture instead of
//! compounding the previous crop's re-encode loss.

use std::fs;

use lingq_upload_lib::commands::project::{set_cover_bytes_impl, set_cover_impl};
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::core::store::{JsonProjectStore, ProjectStore};
use lingq_upload_lib::error::AppError;

/// Store a project whose cover sidecar already holds `bytes`.
fn project_with_cover(
    store: &JsonProjectStore,
    id: &ProjectId,
    bytes: &[u8],
) -> (std::path::PathBuf, std::path::PathBuf) {
    let project_dir = store.project_dir(id).unwrap();
    fs::create_dir_all(&project_dir).unwrap();
    let cover = project_dir.join("cover.jpg");
    fs::write(&cover, bytes).unwrap();
    let mut p = Project::new_test(id.clone(), "T");
    p.cover_path = Some(cover.clone());
    store.put(&p).unwrap();
    (project_dir, cover)
}

#[test]
fn promotes_the_prior_cover_to_original() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let (project_dir, cover) = project_with_cover(&store, &id, b"ORIGINAL");
    store
        .update(&id, &mut |p| {
            p.cover_uploaded_to_lingq = true;
            // A crop must not silently re-enable a cover the user opted out of.
            p.cover_use = false;
        })
        .unwrap();

    let written =
        set_cover_bytes_impl(&store, &id, b"CROPPED".to_vec(), "jpg").expect("set_cover_bytes ok");

    let project = store.get(&id).unwrap().unwrap();
    assert_eq!(written.cover, cover.to_string_lossy());
    assert_eq!(
        written.original.as_deref(),
        Some(&*project_dir.join("cover-original.jpg").to_string_lossy()),
        "the caller is told where the pre-crop image went"
    );
    assert_eq!(project.cover_path.as_deref(), Some(cover.as_path()));
    assert_eq!(fs::read(&cover).unwrap(), b"CROPPED");
    let original = project.cover_original_path.expect("original kept");
    assert_eq!(original, project_dir.join("cover-original.jpg"));
    assert_eq!(fs::read(&original).unwrap(), b"ORIGINAL");
    assert!(!project.cover_uploaded_to_lingq, "upload flag reset");
    assert!(!project.cover_use, "cover_use left alone by a crop");
}

#[test]
fn a_second_crop_keeps_the_first_original() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    project_with_cover(&store, &id, b"ORIGINAL");

    for pass in [&b"CROP1"[..], &b"CROP2"[..]] {
        set_cover_bytes_impl(&store, &id, pass.to_vec(), "jpg").unwrap();
    }

    let project = store.get(&id).unwrap().unwrap();
    let original = project.cover_original_path.expect("original kept");
    assert_eq!(
        fs::read(&original).unwrap(),
        b"ORIGINAL",
        "second crop must re-cut the original, not overwrite it"
    );
    assert_eq!(fs::read(project.cover_path.unwrap()).unwrap(), b"CROP2");
}

#[test]
fn crop_to_a_different_extension_leaves_no_stale_sidecar() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let (project_dir, jpg) = project_with_cover(&store, &id, b"ORIGINAL");

    set_cover_bytes_impl(&store, &id, b"CROPPEDPNG".to_vec(), "png").unwrap();

    assert!(!jpg.exists(), "old .jpg cover removed");
    let project = store.get(&id).unwrap().unwrap();
    assert_eq!(
        project.cover_path.unwrap(),
        project_dir.join("cover.png"),
        "cover now tracked at the new extension"
    );
    assert_eq!(
        fs::read(project_dir.join("cover-original.jpg")).unwrap(),
        b"ORIGINAL",
        "the original keeps its own extension"
    );
}

#[test]
fn picking_a_new_cover_drops_the_stale_original() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let (project_dir, _) = project_with_cover(&store, &id, b"ORIGINAL");
    set_cover_bytes_impl(&store, &id, b"CROPPED".to_vec(), "jpg").unwrap();

    let src_dir = tempfile::tempdir().unwrap();
    let src = src_dir.path().join("fresh.png");
    fs::write(&src, b"FRESH").unwrap();
    set_cover_impl(&store, &id, Some(src.to_string_lossy().into_owned())).unwrap();

    let project = store.get(&id).unwrap().unwrap();
    assert!(project.cover_original_path.is_none());
    assert!(
        !project_dir.join("cover-original.jpg").exists(),
        "stale original deleted with the cover it belonged to"
    );
}

#[test]
fn clearing_the_cover_drops_the_original_too() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let (project_dir, _) = project_with_cover(&store, &id, b"ORIGINAL");
    set_cover_bytes_impl(&store, &id, b"CROPPED".to_vec(), "jpg").unwrap();

    set_cover_impl(&store, &id, None).unwrap();

    let project = store.get(&id).unwrap().unwrap();
    assert!(project.cover_path.is_none());
    assert!(project.cover_original_path.is_none());
    assert!(!project_dir.join("cover-original.jpg").exists());
}

#[test]
fn rejects_an_unsupported_extension() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    store.put(&Project::new_test(id.clone(), "T")).unwrap();

    let err = set_cover_bytes_impl(&store, &id, b"GIF89a".to_vec(), "gif").unwrap_err();
    assert!(
        matches!(err, AppError::Unsupported(ref m) if m.contains("gif")),
        "expected Unsupported for gif, got {err:?}",
    );
}

#[test]
fn rejects_empty_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let (_, cover) = project_with_cover(&store, &id, b"ORIGINAL");

    let err = set_cover_bytes_impl(&store, &id, Vec::new(), "jpg").unwrap_err();

    assert!(matches!(err, AppError::Unsupported(_)), "got {err:?}");
    assert_eq!(
        fs::read(&cover).unwrap(),
        b"ORIGINAL",
        "a rejected save leaves the existing cover untouched"
    );
}
