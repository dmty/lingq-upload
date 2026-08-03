use lingq_upload_lib::lingq::{CollectionId, LanguageCode, LingqClient};
use mockito::Server;
use secrecy::SecretString;

fn ja() -> LanguageCode {
    LanguageCode::new("ja").expect("valid lang")
}

#[tokio::test]
async fn collection_detail_parses_full_record() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/7/")
        .with_status(200)
        .with_body(
            r#"{"id":7,"title":"Kafka on the Shore","description":"A novel",
                "level":"Intermediate 2","duration":22320,
                "lessonsCount":42,"newWordsCount":9204,
                "imageUrl":"https://cdn/x.webp","status":"private",
                "rosesCount":3,"viewsCount":11}"#,
        )
        .create_async()
        .await;
    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());

    let detail = client.collection_detail(CollectionId(7)).await.unwrap();

    assert_eq!(detail.id, 7);
    assert_eq!(detail.title, "Kafka on the Shore");
    assert_eq!(detail.level.as_deref(), Some("Intermediate 2"));
    assert_eq!(detail.duration, Some(22320));
    assert_eq!(detail.lessons_count, Some(42));
    assert_eq!(detail.new_words_count, Some(9204));
}

#[tokio::test]
async fn collection_detail_tolerates_missing_optional_fields() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/8/")
        .with_status(200)
        .with_body(r#"{"pk":8,"title":"Sparse"}"#)
        .create_async()
        .await;
    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());

    let detail = client.collection_detail(CollectionId(8)).await.unwrap();

    assert_eq!(detail.id, 8);
    assert_eq!(detail.title, "Sparse");
    assert_eq!(detail.level, None);
    assert_eq!(detail.duration, None);
    assert_eq!(detail.lessons_count, None);
}

#[tokio::test]
async fn collection_detail_maps_404_to_not_found() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api/v3/ja/collections/99/")
        .with_status(404)
        .with_body(r#"{"detail":"Not found."}"#)
        .create_async()
        .await;
    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());

    let err = client
        .collection_detail(CollectionId(99))
        .await
        .unwrap_err();

    assert!(matches!(err, lingq_upload_lib::lingq::LingqError::NotFound));
}

#[tokio::test]
async fn lesson_stats_project_from_envelope_pages() {
    let mut server = Server::new_async().await;
    let _p1 = server
        .mock(
            "GET",
            "/api/v3/ja/collections/7/lessons/?page=1&page_size=100",
        )
        .with_status(200)
        .with_body(
            r#"{"results":[
                {"id":10,"title":"The Boy Named Crow","duration":512,"wordCount":2841,
                 "uniqueWordCount":900,"newWordsCount":214,"percentCompleted":100.0},
                {"id":11,"title":"Chapter Two","duration":584,"wordCount":3190,
                 "uniqueWordCount":1010,"newWordsCount":287,"percentCompleted":41.5}
            ],"next":null}"#,
        )
        .create_async()
        .await;
    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());

    let stats = client.list_lesson_stats(CollectionId(7)).await.unwrap();

    assert_eq!(stats.len(), 2);
    assert_eq!(stats[0].title, "The Boy Named Crow");
    assert_eq!(stats[0].word_count, Some(2841));
    assert_eq!(stats[0].percent_completed, Some(100.0));
    assert_eq!(stats[1].new_words_count, Some(287));
}

#[tokio::test]
async fn lesson_stats_project_from_bare_array_pages() {
    let mut server = Server::new_async().await;
    let _p1 = server
        .mock(
            "GET",
            "/api/v3/ja/collections/5/lessons/?page=1&page_size=100",
        )
        .with_status(200)
        .with_body(r#"[{"pk":1,"title":"One"}]"#)
        .create_async()
        .await;
    let _p2 = server
        .mock(
            "GET",
            "/api/v3/ja/collections/5/lessons/?page=2&page_size=100",
        )
        .with_status(200)
        .with_body("[]")
        .create_async()
        .await;
    let client = LingqClient::with_base_url(SecretString::new("k".into()), ja(), server.url());

    let stats = client.list_lesson_stats(CollectionId(5)).await.unwrap();

    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].id, 1);
    assert_eq!(stats[0].word_count, None);
    assert_eq!(stats[0].percent_completed, None);
}
