use lingq_upload_lib::lingq::{CollectionId, LanguageCode, LingqClient, LingqError};
use mockito::Server;
use secrecy::SecretString;

fn ja() -> LanguageCode {
    LanguageCode::new("ja").expect("valid lang")
}

fn client(server: &mockito::ServerGuard) -> LingqClient {
    LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url())
}

#[tokio::test]
async fn find_my_collection_matches_exact_title() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/my/?page_size=200&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"next":null,"results":[{"id":7,"title":"Foobar"},{"id":42,"title":"Foo"}]}"#)
        .create_async()
        .await;

    let found = client(&server).find_my_collection("Foo").await.unwrap();
    assert_eq!(found, Some(CollectionId(42)));
}

#[tokio::test]
async fn find_my_collection_matches_nfd_title_to_nfc_request() {
    // LingQ keeps whatever form it was sent; macOS-sourced titles arrive NFD.
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/my/?page_size=200&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("{\"next\":null,\"results\":[{\"id\":11,\"title\":\"\u{30AB}\u{3099}\"}]}")
        .create_async()
        .await;

    let found = client(&server)
        .find_my_collection("\u{30AC}")
        .await
        .unwrap();
    assert_eq!(found, Some(CollectionId(11)));
}

#[tokio::test]
async fn find_my_collection_follows_pages() {
    let mut server = Server::new_async().await;
    let next = format!(
        "{}/api/v3/ja/collections/my/?page=2&page_size=200",
        server.url()
    );
    let _p1 = server
        .mock("GET", "/api/v3/ja/collections/my/?page_size=200&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(
            r#"{{"next":"{next}","results":[{{"id":1,"title":"Other"}}]}}"#
        ))
        .expect(1)
        .create_async()
        .await;
    let _p2 = server
        .mock("GET", "/api/v3/ja/collections/my/?page_size=200&page=2")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"next":null,"results":[{"id":2,"title":"Deep"}]}"#)
        .expect(1)
        .create_async()
        .await;

    let found = client(&server).find_my_collection("Deep").await.unwrap();
    assert_eq!(found, Some(CollectionId(2)));
}

#[tokio::test]
async fn find_my_collection_none_when_absent() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/my/?page_size=200&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"next":null,"results":[{"id":1,"title":"Other"}]}"#)
        .create_async()
        .await;

    assert_eq!(
        client(&server).find_my_collection("Foo").await.unwrap(),
        None
    );
}

#[tokio::test]
async fn find_my_collection_401_is_unauthorized() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/my/?page_size=200&page=1")
        .with_status(401)
        .create_async()
        .await;

    let err = client(&server).find_my_collection("Foo").await.unwrap_err();
    assert!(matches!(err, LingqError::Unauthorized), "{err:?}");
}

#[tokio::test]
async fn create_collection_posts_without_looking_up() {
    let mut server = Server::new_async().await;
    let search = server
        .mock("GET", mockito::Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    let create = server
        .mock("POST", "/api/v3/ja/collections/")
        .with_status(201)
        .with_body(r#"{"pk":777,"title":"Foo"}"#)
        .expect(1)
        .create_async()
        .await;

    let id = client(&server).create_collection("Foo").await.unwrap();
    assert_eq!(id, CollectionId(777));
    search.assert_async().await;
    create.assert_async().await;
}
