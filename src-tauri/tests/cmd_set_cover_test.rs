//! Round-trip test for `cmd_set_cover`. After the cover-image-upload feature,
//! the command copies the source file into the project directory and resets
//! `cover_uploaded_to_lingq`. A `None` value deletes the sidecar.

use std::fs;

use lingq_upload_lib::error::AppError;

use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::core::store::{JsonProjectStore, ProjectStore};

#[test]
fn set_cover_copies_into_project_dir_and_resets_upload_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let mut p = Project::new_test(id.clone(), "T");
    p.cover_uploaded_to_lingq = true;
    store.put(&p).unwrap();

    // Create a source file outside the project dir.
    let src_dir = tempfile::tempdir().unwrap();
    let src = src_dir.path().join("user-pick.png");
    fs::write(&src, b"PNGDATA").unwrap();

    lingq_upload_lib::commands::project::set_cover_impl(
        &store,
        &id,
        Some(src.to_string_lossy().into_owned()),
    )
    .expect("set_cover ok");

    let project = store.get(&id).unwrap().unwrap();
    let cover_path = project.cover_path.expect("cover_path set");
    let project_dir = store.project_dir(&id).unwrap();
    assert!(cover_path.starts_with(&project_dir), "lives inside app-data");
    assert_eq!(cover_path.extension().unwrap().to_string_lossy(), "png");
    assert_eq!(fs::read(&cover_path).unwrap(), b"PNGDATA");
    assert!(!project.cover_uploaded_to_lingq, "upload flag reset");
}

#[test]
fn set_cover_none_deletes_sidecar() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let mut p = Project::new_test(id.clone(), "T");
    // Pre-place a sidecar at the project dir.
    let project_dir = store.project_dir(&id).unwrap();
    fs::create_dir_all(&project_dir).unwrap();
    let sidecar = project_dir.join("cover.jpg");
    fs::write(&sidecar, b"OLD").unwrap();
    p.cover_path = Some(sidecar.clone());
    store.put(&p).unwrap();

    lingq_upload_lib::commands::project::set_cover_impl(&store, &id, None).unwrap();

    let project = store.get(&id).unwrap().unwrap();
    assert!(project.cover_path.is_none());
    assert!(!sidecar.exists(), "sidecar deleted");
}

#[test]
fn set_cover_overwriting_jpg_with_png_removes_old_jpg() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let mut p = Project::new_test(id.clone(), "T");
    let project_dir = store.project_dir(&id).unwrap();
    fs::create_dir_all(&project_dir).unwrap();
    let old = project_dir.join("cover.jpg");
    fs::write(&old, b"OLDJPG").unwrap();
    p.cover_path = Some(old.clone());
    store.put(&p).unwrap();

    let src_dir = tempfile::tempdir().unwrap();
    let src = src_dir.path().join("new.png");
    fs::write(&src, b"NEWPNG").unwrap();

    lingq_upload_lib::commands::project::set_cover_impl(
        &store,
        &id,
        Some(src.to_string_lossy().into_owned()),
    )
    .unwrap();

    assert!(!old.exists(), "old .jpg sidecar removed");
    let project = store.get(&id).unwrap().unwrap();
    let new_cover = project.cover_path.unwrap();
    assert_eq!(new_cover.extension().unwrap().to_string_lossy(), "png");
}

#[test]
fn set_cover_extensionless_path_returns_unsupported() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let id = ProjectId::from_title_author("T", "Author");
    let p = Project::new_test(id.clone(), "T");
    store.put(&p).unwrap();

    let err = lingq_upload_lib::commands::project::set_cover_impl(
        &store,
        &id,
        Some("/no/ext/file".into()),
    )
    .unwrap_err();
    assert!(
        matches!(err, AppError::Unsupported(ref m) if m.contains("no extension")),
        "expected Unsupported with no-extension message, got {err:?}",
    );
}
