use lingq_upload_lib::lingq::{CollectionId, LingqClient};
use mockito::Server;
use secrecy::SecretString;

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

    let client = LingqClient::with_base_url(
        SecretString::new("k".into()),
        "ja",
        server.url(),
    );
    let id = client
        .find_or_create_collection("Foo", "desc", "ja")
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
    let client = LingqClient::with_base_url(
        SecretString::new("k".into()),
        "ja",
        server.url(),
    );
    let id = client
        .find_or_create_collection("Foo", "desc", "ja")
        .await
        .unwrap();
    assert_eq!(id, CollectionId(777));
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
    let client = LingqClient::with_base_url(
        SecretString::new("k".into()),
        "ja",
        server.url(),
    );
    let err = client
        .find_or_create_collection("Foo", "desc", "ja")
        .await
        .unwrap_err();
    matches!(err, lingq_upload_lib::lingq::LingqError::Unauthorized);
}
