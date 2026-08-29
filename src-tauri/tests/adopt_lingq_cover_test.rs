//! When a project has a LingQ collection id but no local cover, fetching the
//! course (which already returns `imageUrl`) must copy that image into the
//! project dir and set `cover_path`. Library reads `cover_path`; a remote
//! `<img src>` on the course screen is not persistence.

use lingq_upload_lib::commands::lingq::adopt_missing_cover;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::core::store::{JsonProjectStore, ProjectStore};
use mockito::Server;

fn png() -> Vec<u8> {
    // 1×1 PNG
    vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ]
}

fn project_for(collection_id: i64, cover: Option<std::path::PathBuf>) -> Project {
    let id = ProjectId::from_title_author("Toki", "Author");
    let mut p = Project::new_test(id, "時をかける少女");
    p.lingq_collection_id = Some(collection_id);
    p.cover_path = cover;
    p
}

#[tokio::test]
async fn adopt_saves_lingq_image_when_cover_path_missing() {
    let mut server = Server::new_async().await;
    let body = png();
    let _m = server
        .mock("GET", "/covers/toki.png")
        .with_status(200)
        .with_header("content-type", "image/png")
        .with_body(&body)
        .create_async()
        .await;
    let url = format!("{}/covers/toki.png", server.url());

    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let project = project_for(2792683, None);
    let id = project.id.clone();
    store.put(&project).unwrap();

    let n = adopt_missing_cover(&store, &reqwest::Client::new(), 2792683, &url)
        .await
        .expect("adopt ok");
    assert_eq!(n, 1);

    let after = store.get(&id).unwrap().unwrap();
    let cover_path = after.cover_path.expect("cover_path set");
    assert!(cover_path.starts_with(store.project_dir(&id).unwrap()));
    assert_eq!(cover_path.extension().unwrap().to_string_lossy(), "png");
    assert_eq!(std::fs::read(&cover_path).unwrap(), body);
    assert!(
        after.cover_uploaded_to_lingq,
        "already on LingQ; do not re-upload"
    );
}

#[tokio::test]
async fn adopt_leaves_existing_cover_alone() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/covers/toki.png")
        .with_status(200)
        .with_header("content-type", "image/png")
        .with_body(png())
        .expect(0)
        .create_async()
        .await;
    let url = format!("{}/covers/toki.png", server.url());

    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let dir = store
        .project_dir(&ProjectId::from_title_author("Toki", "Author"))
        .unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let existing = dir.join("cover.jpg");
    std::fs::write(&existing, b"LOCAL").unwrap();

    let project = project_for(2792683, Some(existing.clone()));
    store.put(&project).unwrap();

    let n = adopt_missing_cover(&store, &reqwest::Client::new(), 2792683, &url)
        .await
        .expect("adopt ok");
    assert_eq!(n, 0);
    assert_eq!(std::fs::read(&existing).unwrap(), b"LOCAL");
}

#[tokio::test]
async fn adopt_download_failure_is_soft() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/covers/missing.png")
        .with_status(404)
        .create_async()
        .await;
    let url = format!("{}/covers/missing.png", server.url());

    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let project = project_for(1, None);
    let id = project.id.clone();
    store.put(&project).unwrap();

    let n = adopt_missing_cover(&store, &reqwest::Client::new(), 1, &url)
        .await
        .expect("404 is not a hard error");
    assert_eq!(n, 0);
    assert!(store.get(&id).unwrap().unwrap().cover_path.is_none());
}
