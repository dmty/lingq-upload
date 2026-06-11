//! Per-language URL threading. Spins up mockito, builds a `LingqClient` with
//! a non-`ja` language, calls `find_or_create_collection`, and asserts the
//! request URL carries the matching segment. Covers ko / ru / en.

use lingq_upload_lib::lingq::{
    CollectionId, ImportLessonRequest, LanguageCode, LessonStatus, LingqClient,
};
use mockito::{Matcher, Server};
use secrecy::SecretString;

async fn assert_create_uses_lang_segment(code: &str) {
    let mut server = Server::new_async().await;

    let search_path = format!("/api/v3/{code}/collections/?search=Sample");
    let create_path = format!("/api/v3/{code}/collections/");

    let search = server
        .mock("GET", search_path.as_str())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results":[]}"#)
        .expect(1)
        .create_async()
        .await;
    let create = server
        .mock("POST", create_path.as_str())
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"pk":4242,"title":"Sample"}"#)
        .expect(1)
        .create_async()
        .await;

    let lang = LanguageCode::new(code).expect("valid lang");
    let client = LingqClient::with_base_url(
        SecretString::from("test-key".to_string()),
        lang,
        server.url(),
    );

    let id = client
        .find_or_create_collection("Sample", "desc")
        .await
        .expect("create ok");
    assert_eq!(id, CollectionId(4242));

    search.assert_async().await;
    create.assert_async().await;
}

#[tokio::test]
async fn create_collection_uses_ko_url_segment() {
    assert_create_uses_lang_segment("ko").await;
}

#[tokio::test]
async fn create_collection_uses_ru_url_segment() {
    assert_create_uses_lang_segment("ru").await;
}

#[tokio::test]
async fn create_collection_uses_en_url_segment() {
    assert_create_uses_lang_segment("en").await;
}

#[tokio::test]
async fn import_lesson_v2_threads_ko_through_url_and_form_field() {
    let mut server = Server::new_async().await;

    // URL segment AND multipart `language` field must both be "ko".
    let body_matcher = Matcher::AllOf(vec![
        Matcher::Regex(r#"name="language"\r\n\r\nko\r\n"#.to_string()),
        Matcher::Regex(r#"name="title"\r\n\r\nKo Chapter\r\n"#.to_string()),
    ]);
    let m = server
        .mock("POST", "/api/v3/ko/lessons/import/")
        .match_body(body_matcher)
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"pk":777}"#)
        .expect(1)
        .create_async()
        .await;

    let lang = LanguageCode::new("ko").expect("valid lang");
    let client = LingqClient::with_base_url(
        SecretString::from("test-key".to_string()),
        lang,
        server.url(),
    );

    let req = ImportLessonRequest {
        collection: CollectionId(11),
        title: "Ko Chapter",
        text: "annyeong",
        audio: None,
        level: 1,
        status: LessonStatus::Private,
        tags: &[],
        save: true,
    };
    let id = client.import_lesson_v2(req).await.expect("import ok");
    assert_eq!(id, 777);
    m.assert_async().await;
}
