use lingq_upload_lib::lingq::{CollectionId, LingqClient};
use mockito::Server;
use secrecy::SecretString;

#[tokio::test]
async fn list_lessons_paginates_results() {
    let mut server = Server::new_async().await;
    let _p1 = server
        .mock(
            "GET",
            "/api/v3/ja/collections/1/lessons/?page=1&page_size=100",
        )
        .with_status(200)
        .with_body(r#"{"results":[{"pk":10,"title":"A"},{"pk":11,"title":"B"}],"next":"next-page"}"#)
        .create_async()
        .await;
    let _p2 = server
        .mock(
            "GET",
            "/api/v3/ja/collections/1/lessons/?page=2&page_size=100",
        )
        .with_status(200)
        .with_body(r#"{"results":[{"pk":12,"title":"C"}],"next":null}"#)
        .create_async()
        .await;
    let client = LingqClient::with_base_url(
        SecretString::new("k".into()),
        "ja",
        server.url(),
    );
    let lessons = client.list_lessons(CollectionId(1)).await.unwrap();
    assert_eq!(lessons.len(), 3);
    assert_eq!(lessons[0].title, "A");
    assert_eq!(lessons[2].id, 12);
}
