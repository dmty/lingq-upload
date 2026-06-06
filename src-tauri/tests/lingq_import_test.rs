use std::io::Write;

use lingq_upload_lib::lingq::{CollectionId, ImportLessonRequest, LessonStatus, LingqClient};
use mockito::Server;
use secrecy::SecretString;

#[tokio::test]
async fn import_lesson_v2_posts_multipart_and_parses_id() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"pk":999,"title":"x"}"#)
        .create_async()
        .await;

    let client = LingqClient::with_base_url(
        SecretString::new("k".into()),
        "en", // client-global lang different from request lang
        server.url(),
    );

    let mut audio = tempfile::Builder::new()
        .suffix(".mp3")
        .tempfile()
        .unwrap();
    audio.write_all(b"fake mp3 bytes").unwrap();

    let tags = ["a", "b"];
    let req = ImportLessonRequest {
        collection: CollectionId(1),
        title: "Chapter 1",
        text: "Hello world",
        audio: Some(audio.path()),
        language: "ja",
        level: 2,
        status: LessonStatus::Private,
        tags: &tags,
        save: true,
    };
    let id = client.import_lesson_v2(req).await.unwrap();
    assert_eq!(id, 999);
}

#[tokio::test]
async fn import_lesson_v2_retries_on_5xx_then_succeeds() {
    let mut server = Server::new_async().await;
    let _fail = server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(503)
        .expect(2)
        .create_async()
        .await;
    let _ok = server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(201)
        .with_body(r#"{"pk":42}"#)
        .create_async()
        .await;
    let client = LingqClient::with_base_url(
        SecretString::new("k".into()),
        "en",
        server.url(),
    );
    let req = ImportLessonRequest {
        collection: CollectionId(1),
        title: "Chapter",
        text: "hi",
        audio: None,
        language: "ja",
        level: 1,
        status: LessonStatus::Private,
        tags: &[],
        save: true,
    };
    let id = client.import_lesson_v2(req).await.unwrap();
    assert_eq!(id, 42);
}

#[tokio::test]
async fn import_lesson_v2_exhausts_three_attempts_on_5xx() {
    let mut server = Server::new_async().await;
    let _fail = server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(503)
        .expect(3)
        .create_async()
        .await;
    let client = LingqClient::with_base_url(
        SecretString::new("k".into()),
        "en",
        server.url(),
    );
    let req = ImportLessonRequest {
        collection: CollectionId(1),
        title: "Chapter",
        text: "hi",
        audio: None,
        language: "ja",
        level: 1,
        status: LessonStatus::Private,
        tags: &[],
        save: true,
    };
    let err = client.import_lesson_v2(req).await.unwrap_err();
    assert!(
        matches!(err, lingq_upload_lib::lingq::LingqError::Server(_)),
        "got {err:?}"
    );
}

#[tokio::test]
async fn import_lesson_v2_4xx_fails_fast() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(400)
        .with_body("bad")
        .expect(1) // must NOT retry on 4xx
        .create_async()
        .await;
    let client = LingqClient::with_base_url(
        SecretString::new("k".into()),
        "en",
        server.url(),
    );
    let req = ImportLessonRequest {
        collection: CollectionId(1),
        title: "Chapter",
        text: "hi",
        audio: None,
        language: "ja",
        level: 1,
        status: LessonStatus::Private,
        tags: &[],
        save: true,
    };
    let err = client.import_lesson_v2(req).await.unwrap_err();
    assert!(
        matches!(err, lingq_upload_lib::lingq::LingqError::BadRequest(_)),
        "got {err:?}"
    );
}
