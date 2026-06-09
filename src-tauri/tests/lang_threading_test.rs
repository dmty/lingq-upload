//! Per-language URL threading. Spins up mockito, builds a `LingqClient` with
//! a non-`ja` language, calls `find_or_create_collection`, and asserts the
//! request URL carries the matching segment. Covers ko / ru / en.

use lingq_upload_lib::lingq::{CollectionId, LanguageCode, LingqClient};
use mockito::Server;
use secrecy::SecretString;

async fn assert_create_uses_lang_segment(code: &str) {
    let mut server = Server::new_async().await;

    let search_path = format!("/api/v3/{code}/collections/?search=Sample");
    let create_path = format!("/api/v3/{code}/collections/");

    let _search = server
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
    let client =
        LingqClient::with_base_url(SecretString::from("test-key".to_string()), lang, server.url());

    let id = client
        .find_or_create_collection("Sample", "desc")
        .await
        .expect("create ok");
    assert_eq!(id, CollectionId(4242));

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
