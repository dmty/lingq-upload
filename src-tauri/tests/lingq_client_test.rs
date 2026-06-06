//! Integration tests for `LingqClient`. Plays JSON cassettes against a local
//! mockito server. No live network. See `tests/fixtures/lingq/cassettes/*.json`.

use std::io::Write;
use std::path::PathBuf;

use lingq_upload_lib::lingq::{Collection, Language, LessonOpts, LingqClient, LingqError};
use mockito::{Matcher, Server, ServerGuard};
use secrecy::SecretString;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Cassette {
    method: String,
    url_path: String,
    #[serde(default)]
    url_query: String,
    status: u16,
    response_body: String,
    response_content_type: String,
}

fn load_cassette(name: &str) -> Cassette {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lingq/cassettes")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cassette {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse cassette {name}: {e}"))
}

async fn spawn_server() -> ServerGuard {
    Server::new_async().await
}

fn client_for(server: &ServerGuard, lang: &str, token: &str) -> LingqClient {
    LingqClient::with_base_url(SecretString::from(token.to_string()), lang, server.url())
}

fn write_tmp_mp3() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("audio.mp3");
    let mut f = std::fs::File::create(&path).expect("create mp3");
    // ID3v2 magic + minimal payload — content doesn't matter to mockito.
    f.write_all(b"ID3\x03\x00\x00\x00\x00\x00\x00fakeaudio")
        .expect("write");
    (dir, path)
}

#[tokio::test]
async fn whoami_200_returns_ok() {
    let cas = load_cassette("whoami_200.json");
    let mut server = spawn_server().await;

    let _m = server
        .mock(&cas.method, cas.url_path.as_str())
        .match_query(Matcher::UrlEncoded("page_size".into(), "1".into()))
        .match_header("authorization", "Token test-key-200")
        .with_status(cas.status as usize)
        .with_header("content-type", &cas.response_content_type)
        .with_body(&cas.response_body)
        .create_async()
        .await;

    let client = client_for(&server, "ja", "test-key-200");
    let res = client.whoami().await.expect("whoami ok");
    assert!(res.ok);
}

#[tokio::test]
async fn whoami_401_returns_unauthorized() {
    let cas = load_cassette("whoami_401.json");
    let mut server = spawn_server().await;

    let _m = server
        .mock(&cas.method, cas.url_path.as_str())
        .match_query(Matcher::UrlEncoded("page_size".into(), "1".into()))
        .with_status(cas.status as usize)
        .with_header("content-type", &cas.response_content_type)
        .with_body(&cas.response_body)
        .create_async()
        .await;

    let client = client_for(&server, "ja", "wrong-key");
    let err = client.whoami().await.expect_err("should error");
    assert!(matches!(err, LingqError::Unauthorized));
}

#[tokio::test]
async fn whoami_404_lang_mismatch_returns_not_found() {
    let cas = load_cassette("whoami_404.json");
    let mut server = spawn_server().await;

    let _m = server
        .mock(&cas.method, cas.url_path.as_str())
        .match_query(Matcher::UrlEncoded("page_size".into(), "1".into()))
        .with_status(cas.status as usize)
        .with_header("content-type", &cas.response_content_type)
        .with_body(&cas.response_body)
        .create_async()
        .await;

    let client = client_for(&server, "zz", "test-key");
    let err = client.whoami().await.expect_err("should error");
    assert!(matches!(err, LingqError::NotFound));
}

#[tokio::test]
async fn import_lesson_200_returns_id() {
    let cas = load_cassette("import_lesson_201.json");
    let mut server = spawn_server().await;

    let _m = server
        .mock(&cas.method, cas.url_path.as_str())
        .match_header(
            "content-type",
            Matcher::Regex("multipart/form-data".into()),
        )
        .with_status(cas.status as usize)
        .with_header("content-type", &cas.response_content_type)
        .with_body(&cas.response_body)
        .create_async()
        .await;

    let (_dir, mp3) = write_tmp_mp3();
    let client = client_for(&server, "ja", "test-key");
    let id = client
        .import_lesson(42, "Chapter 1", "hello world", &mp3, &LessonOpts::default())
        .await
        .expect("import ok");
    assert_eq!(id, 987654);
}

#[tokio::test]
async fn import_lesson_400_returns_bad_request_with_detail() {
    let cas = load_cassette("import_lesson_400.json");
    let mut server = spawn_server().await;

    let _m = server
        .mock(&cas.method, cas.url_path.as_str())
        .with_status(cas.status as usize)
        .with_header("content-type", &cas.response_content_type)
        .with_body(&cas.response_body)
        .create_async()
        .await;

    let (_dir, mp3) = write_tmp_mp3();
    let client = client_for(&server, "ja", "test-key");
    let err = client
        .import_lesson(42, "Chapter 1", "", &mp3, &LessonOpts::default())
        .await
        .expect_err("should error");
    match err {
        LingqError::BadRequest(detail) => {
            assert!(
                detail.contains("text field is required"),
                "expected detail forwarded, got {detail}"
            );
        }
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[tokio::test]
async fn import_lesson_response_missing_id_returns_schema_error() {
    let cas = load_cassette("import_lesson_missing_id.json");
    let mut server = spawn_server().await;

    let _m = server
        .mock(&cas.method, cas.url_path.as_str())
        .with_status(cas.status as usize)
        .with_header("content-type", &cas.response_content_type)
        .with_body(&cas.response_body)
        .create_async()
        .await;

    let (_dir, mp3) = write_tmp_mp3();
    let client = client_for(&server, "ja", "test-key");
    let err = client
        .import_lesson(42, "Chapter 1", "x", &mp3, &LessonOpts::default())
        .await
        .expect_err("should error");
    assert!(matches!(err, LingqError::Schema(_)), "got {err:?}");
}

#[tokio::test]
async fn import_lesson_response_with_extra_fields_succeeds() {
    // AC6: extra/unknown fields are tolerated; created_at format drift OK.
    let mut server = spawn_server().await;
    let body = r#"{
        "id": 12345,
        "title": "Chapter 1",
        "created_at": "2026/15/01 funky-format",
        "future_field": {"nested": true},
        "tags_resolved": ["books", "import"]
    }"#;

    let _m = server
        .mock("POST", "/api/v3/ja/lessons/import/")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;

    let (_dir, mp3) = write_tmp_mp3();
    let client = client_for(&server, "ja", "test-key");
    let id = client
        .import_lesson(42, "Chapter 1", "x", &mp3, &LessonOpts::default())
        .await
        .expect("extras tolerated");
    assert_eq!(id, 12345);
}

#[tokio::test]
async fn list_languages_200_parses_flat_array() {
    let cas = load_cassette("list_languages_200.json");
    let mut server = spawn_server().await;

    let _m = server
        .mock(&cas.method, cas.url_path.as_str())
        .match_header("authorization", "Token test-key")
        .with_status(cas.status as usize)
        .with_header("content-type", &cas.response_content_type)
        .with_body(&cas.response_body)
        .create_async()
        .await;

    let client = client_for(&server, "en", "test-key");
    let langs: Vec<Language> = client.list_my_languages().await.expect("languages ok");
    assert_eq!(langs.len(), 4);
    let ja = langs.iter().find(|l| l.code == "ja").expect("ja entry");
    assert_eq!(ja.title, "Japanese");
    assert_eq!(ja.known_words, 73900);
}

#[tokio::test]
async fn list_languages_tolerates_alt_field_names() {
    // LingQ has shipped this payload under different field names over the years;
    // tolerate english_name / wordsKnown / language slug spellings.
    let mut server = spawn_server().await;
    let body = r#"[
        {"language":"ja","english_name":"Japanese","wordsKnown":50000},
        {"code":"fr","name":"French","known_words":100}
    ]"#;
    let _m = server
        .mock("GET", "/api/v2/languages/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;

    let client = client_for(&server, "en", "test-key");
    let langs = client.list_my_languages().await.expect("ok");
    assert_eq!(langs.len(), 2);
    assert!(langs.iter().any(|l| l.code == "ja" && l.known_words == 50000));
    assert!(langs.iter().any(|l| l.code == "fr" && l.title == "French"));
}

#[tokio::test]
async fn list_collections_200_parses_results_wrapper() {
    let cas = load_cassette("list_collections_200.json");
    let mut server = spawn_server().await;

    let _m = server
        .mock(&cas.method, cas.url_path.as_str())
        .match_query(Matcher::UrlEncoded("page_size".into(), "200".into()))
        .with_status(cas.status as usize)
        .with_header("content-type", &cas.response_content_type)
        .with_body(&cas.response_body)
        .create_async()
        .await;

    let client = client_for(&server, "ja", "test-key");
    let cols: Vec<Collection> = client.list_my_collections().await.expect("collections ok");
    assert_eq!(cols.len(), 2);
    assert!(cols.iter().any(|c| c.id == 123456 && c.title == "海辺のカフカ"));
}

#[tokio::test]
async fn auth_header_present_and_starts_with_token() {
    let mut server = spawn_server().await;

    let m = server
        .mock("GET", "/api/v3/ja/collections/my/")
        .match_query(Matcher::UrlEncoded("page_size".into(), "1".into()))
        .match_header(
            "authorization",
            Matcher::Regex("^Token [A-Za-z0-9._-]+$".into()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"count":0,"results":[]}"#)
        .create_async()
        .await;

    let client = client_for(&server, "ja", "abc.DEF-123_xyz");
    let _ = client.whoami().await.expect("whoami ok");
    m.assert_async().await;
}
