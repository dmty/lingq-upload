use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

use lingq_upload_lib::error::AppError;
use lingq_upload_lib::transcribe::{
    ProviderCatalog, ProviderDescriptor, TranscribeError, TranscribeErrorKind, TranscribeOpts,
    TranscribeProviderId, Transcriber, WhisperLikeTranscriber,
};
use mockito::{Matcher, Server};
use reqwest::Client;
use secrecy::SecretString;

fn audio_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let audio = dir.path().join("clip.wav");
    std::fs::write(&audio, b"audio-bytes").expect("write audio fixture");
    (dir, audio)
}

fn descriptor(id: TranscribeProviderId) -> &'static ProviderDescriptor {
    ProviderCatalog::built_in().descriptor(id).unwrap()
}

fn transcriber(
    descriptor: &'static ProviderDescriptor,
    key: &str,
    http: Client,
    endpoint: String,
) -> WhisperLikeTranscriber {
    WhisperLikeTranscriber::with_endpoint(
        descriptor,
        SecretString::from(key.to_owned()),
        http,
        endpoint,
    )
}

fn multipart_field(name: &str, value: &str) -> Matcher {
    Matcher::Regex(format!(r#"name="{name}"[\s\S]*?\r\n\r\n{value}\r\n"#))
}

#[test]
fn operational_transcribe_errors_have_typed_serialized_kinds() {
    for (kind, serialized_kind) in [
        (TranscribeErrorKind::ApiKey, "api_key"),
        (TranscribeErrorKind::Unauthorized, "unauthorized"),
        (TranscribeErrorKind::RateLimit, "rate_limit"),
        (TranscribeErrorKind::Timeout, "timeout"),
        (TranscribeErrorKind::Network, "network"),
        (TranscribeErrorKind::ProviderFailed, "provider_failed"),
        (TranscribeErrorKind::Audio, "audio"),
    ] {
        let error = TranscribeError {
            kind,
            message: "safe detail".into(),
        };
        let app_error = AppError::from(error);

        assert_eq!(
            serde_json::to_value(app_error).unwrap(),
            serde_json::json!({
                "kind": "Transcribe",
                "message": {
                    "kind": serialized_kind,
                    "message": "safe detail",
                },
            })
        );
    }
}

#[tokio::test]
async fn groq_posts_text_multipart_with_optional_fields() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/openai/v1/audio/transcriptions")
        .match_header("authorization", "Bearer test-secret")
        .match_header("content-type", Matcher::Regex("multipart/form-data".into()))
        .match_body(Matcher::Regex(
            r#"name="file"; filename="clip\.wav"[\s\S]*audio-bytes"#.into(),
        ))
        .match_body(multipart_field("model", "whisper-large-v3-turbo"))
        .match_body(multipart_field("response_format", "text"))
        .match_body(multipart_field("language", "ja"))
        .match_body(multipart_field("prompt", "A proper noun"))
        .with_status(200)
        .with_body(" \nrecognized words\t ")
        .create_async()
        .await;
    let (_dir, audio) = audio_fixture();
    let client = transcriber(
        descriptor(TranscribeProviderId::Groq),
        "test-secret",
        Client::new(),
        format!("{}/openai/v1/audio/transcriptions", server.url()),
    );

    let transcript = client
        .transcribe(
            &audio,
            &TranscribeOpts {
                language: Some("ja".into()),
                prompt: Some("A proper noun".into()),
            },
        )
        .await
        .expect("text response");

    assert_eq!(transcript.text, "recognized words");
    mock.assert_async().await;
}

#[tokio::test]
async fn openai_omits_absent_optional_fields() {
    let mut server = Server::new_async().await;
    let captured_body = Arc::new(Mutex::new(None));
    let body_for_mock = Arc::clone(&captured_body);
    let mock = server
        .mock("POST", "/v1/audio/transcriptions")
        .match_header("authorization", "Bearer openai-secret")
        .match_body(multipart_field("model", "whisper-1"))
        .match_body(multipart_field("response_format", "text"))
        .with_status(200)
        .with_body_from_request(move |request| {
            *body_for_mock.lock().unwrap() = Some(request.utf8_lossy_body().unwrap().into_owned());
            b"openai words".to_vec()
        })
        .create_async()
        .await;
    let (_dir, audio) = audio_fixture();
    let client = transcriber(
        descriptor(TranscribeProviderId::OpenAi),
        "openai-secret",
        Client::new(),
        format!("{}/v1/audio/transcriptions", server.url()),
    );

    let transcript = client
        .transcribe(&audio, &TranscribeOpts::default())
        .await
        .expect("text response");

    assert_eq!(transcript.text, "openai words");
    mock.assert_async().await;
    let body = captured_body.lock().unwrap();
    let body = body.as_ref().expect("captured multipart body");
    assert!(!body.contains("name=\"language\""));
    assert!(!body.contains("name=\"prompt\""));
}

#[tokio::test]
async fn empty_success_body_returns_an_empty_transcript() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/openai/v1/audio/transcriptions")
        .with_status(200)
        .with_body("")
        .create_async()
        .await;
    let (_dir, audio) = audio_fixture();
    let client = transcriber(
        descriptor(TranscribeProviderId::Groq),
        "test-secret",
        Client::new(),
        format!("{}/openai/v1/audio/transcriptions", server.url()),
    );

    let transcript = client
        .transcribe(&audio, &TranscribeOpts::default())
        .await
        .expect("empty transcript is a content outcome");

    assert!(transcript.text.is_empty());
    mock.assert_async().await;
}

#[tokio::test]
async fn provider_statuses_map_to_typed_scrubbed_errors() {
    for (status, expected_kind) in [
        (401, TranscribeErrorKind::Unauthorized),
        (403, TranscribeErrorKind::Unauthorized),
        (429, TranscribeErrorKind::RateLimit),
        (500, TranscribeErrorKind::ProviderFailed),
    ] {
        let mut server = Server::new_async().await;
        let response_body = format!("Bearer secret-value {}", "界".repeat(980));
        assert_eq!(response_body.chars().count(), 1_000);
        let mock = server
            .mock("POST", "/openai/v1/audio/transcriptions")
            .match_header("authorization", "Bearer request-secret-456")
            .with_status(status)
            .with_body(response_body)
            .create_async()
            .await;
        let (_dir, audio) = audio_fixture();
        let client = transcriber(
            descriptor(TranscribeProviderId::Groq),
            "request-secret-456",
            Client::new(),
            format!("{}/openai/v1/audio/transcriptions", server.url()),
        );

        let error = client
            .transcribe(&audio, &TranscribeOpts::default())
            .await
            .expect_err("provider status must fail");

        assert_eq!(error.kind(), expected_kind, "status {status}");
        let message = error.to_string();
        assert!(!message.contains("request-secret-456"), "status {status}");
        assert!(!message.contains("secret-value"), "status {status}");
        let max_message_scalars =
            format!("provider returned HTTP {status}: ").chars().count() + 512;
        assert!(
            message.chars().count() <= max_message_scalars,
            "status {status}: {message}"
        );
        mock.assert_async().await;
    }
}

#[tokio::test]
async fn delayed_response_maps_to_timeout() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/openai/v1/audio/transcriptions")
        .with_status(200)
        .with_chunked_body(|writer| {
            std::thread::sleep(Duration::from_millis(100));
            writer.write_all(b"too late")
        })
        .create_async()
        .await;
    let (_dir, audio) = audio_fixture();
    let http = Client::builder()
        .connect_timeout(Duration::from_millis(20))
        .timeout(Duration::from_millis(20))
        .build()
        .unwrap();
    let client = transcriber(
        descriptor(TranscribeProviderId::Groq),
        "test-secret",
        http,
        format!("{}/openai/v1/audio/transcriptions", server.url()),
    );

    let error = client
        .transcribe(&audio, &TranscribeOpts::default())
        .await
        .expect_err("delayed response must time out");

    assert_eq!(error.kind(), TranscribeErrorKind::Timeout);
    assert!(!error.to_string().contains("test-secret"));
    mock.assert_async().await;
}

#[tokio::test]
async fn response_headers_after_connect_timeout_are_accepted() {
    let mut server = Server::new_async().await;
    let request_seen = Arc::new(tokio::sync::Notify::new());
    let seen_for_mock = Arc::clone(&request_seen);
    let response_gate = Arc::new(Barrier::new(2));
    let gate_for_mock = Arc::clone(&response_gate);
    let mock = server
        .mock("POST", "/openai/v1/audio/transcriptions")
        .with_status(200)
        .with_body_from_request(move |_| {
            seen_for_mock.notify_one();
            gate_for_mock.wait();
            b"eventual words".to_vec()
        })
        .create_async()
        .await;
    let (dir, audio) = audio_fixture();
    let http = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();
    let client = transcriber(
        descriptor(TranscribeProviderId::Groq),
        "test-secret",
        http,
        format!("{}/openai/v1/audio/transcriptions", server.url()),
    );
    let transcription = tokio::spawn(async move {
        let _dir = dir;
        client.transcribe(&audio, &TranscribeOpts::default()).await
    });

    request_seen.notified().await;
    tokio::time::pause();
    let release_response = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(11)).await;
        response_gate.wait();
        tokio::time::resume();
    });

    let result = transcription.await.expect("transcription task");
    release_response.await.expect("response release task");
    let transcript = result.expect("response within the overall timeout");
    assert_eq!(transcript.text, "eventual words");
    mock.assert_async().await;
}

#[tokio::test]
async fn connection_refusal_maps_to_network() {
    let (_dir, audio) = audio_fixture();
    let http = Client::builder()
        .connect_timeout(Duration::from_millis(50))
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let client = transcriber(
        descriptor(TranscribeProviderId::Groq),
        "connection-secret",
        http,
        "http://127.0.0.1:0/openai/v1/audio/transcriptions".into(),
    );

    let error = client
        .transcribe(&audio, &TranscribeOpts::default())
        .await
        .expect_err("closed local endpoint must fail");

    assert_eq!(error.kind(), TranscribeErrorKind::Network);
    assert!(!error.to_string().contains("connection-secret"));
}

#[tokio::test]
async fn unreadable_audio_maps_to_audio_without_network_io() {
    let server = Server::new_async().await;
    let client = transcriber(
        descriptor(TranscribeProviderId::Groq),
        "audio-secret",
        Client::new(),
        format!("{}/openai/v1/audio/transcriptions", server.url()),
    );

    let error = client
        .transcribe(
            std::path::Path::new("definitely-not-a-real-audio-file.mp3"),
            &TranscribeOpts::default(),
        )
        .await
        .expect_err("missing audio must fail");

    assert_eq!(error.kind(), TranscribeErrorKind::Audio);
    assert!(!error.to_string().contains("audio-secret"));
}
