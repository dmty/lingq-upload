use lingq_upload_lib::lingq::{CollectionId, LanguageCode, LingqClient};
use mockito::Server;
use secrecy::SecretString;

fn ja() -> LanguageCode {
    LanguageCode::new("ja").expect("valid lang")
}

#[tokio::test]
async fn find_or_create_returns_existing_id_on_exact_match() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/?search=Foo")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results":[{"pk":42,"title":"Foo"}]}"#)
        .create_async()
        .await;

    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());
    let id = client
        .find_or_create_collection("Foo", "desc")
        .await
        .unwrap();
    assert_eq!(id, CollectionId(42));
}

#[tokio::test]
async fn find_or_create_posts_when_no_match() {
    let mut server = Server::new_async().await;
    let _search = server
        .mock("GET", "/api/v3/ja/collections/?search=Foo")
        .with_status(200)
        .with_body(r#"{"results":[]}"#)
        .create_async()
        .await;
    let _create = server
        .mock("POST", "/api/v3/ja/collections/")
        .with_status(201)
        .with_body(r#"{"pk":777,"title":"Foo"}"#)
        .create_async()
        .await;
    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());
    let id = client
        .find_or_create_collection("Foo", "desc")
        .await
        .unwrap();
    assert_eq!(id, CollectionId(777));
}

#[tokio::test]
async fn find_or_create_matches_nfd_response_to_nfc_request() {
    // Request "ガ" (NFC, precomposed U+30AC); server returns NFD form
    // (U+30AB KATAKANA KA + U+3099 voicing). Should match via title_hash.
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/?search=%E3%82%AC")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("{\"results\":[{\"pk\":11,\"title\":\"\u{30AB}\u{3099}\"}]}")
        .create_async()
        .await;

    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());
    let id = client
        .find_or_create_collection("\u{30AC}", "desc")
        .await
        .unwrap();
    assert_eq!(id, CollectionId(11));
}

#[tokio::test]
async fn find_or_create_post_4xx_then_research_finds_id() {
    // Race: search misses, POST returns 400 ("title already exists"),
    // a follow-up search now finds the row.
    let mut server = Server::new_async().await;
    let _search_first = server
        .mock("GET", "/api/v3/ja/collections/?search=Race")
        .with_status(200)
        .with_body(r#"{"results":[]}"#)
        .expect(1)
        .create_async()
        .await;
    let _create = server
        .mock("POST", "/api/v3/ja/collections/")
        .with_status(400)
        .with_body("title already exists")
        .expect(1)
        .create_async()
        .await;
    let _search_second = server
        .mock("GET", "/api/v3/ja/collections/?search=Race")
        .with_status(200)
        .with_body(r#"{"results":[{"pk":555,"title":"Race"}]}"#)
        .expect(1)
        .create_async()
        .await;
    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());
    let id = client
        .find_or_create_collection("Race", "desc")
        .await
        .unwrap();
    assert_eq!(id, CollectionId(555));
}

#[tokio::test]
async fn find_or_create_401_returns_unauthorized() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/?search=Foo")
        .with_status(401)
        .with_body("Unauthorized")
        .create_async()
        .await;
    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());
    let err = client
        .find_or_create_collection("Foo", "desc")
        .await
        .unwrap_err();
    matches!(err, lingq_upload_lib::lingq::LingqError::Unauthorized);
}
