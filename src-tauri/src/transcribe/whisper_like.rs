use std::path::Path;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::StreamExt;
use reqwest::header::CONTENT_TYPE;
use reqwest::multipart::Form;
use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};

use super::provider::ProviderDescriptor;
use super::{
    TranscribeError, TranscribeErrorKind, TranscribeOpts, TranscribeProviderId, Transcriber,
    Transcript,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const ERROR_EXCERPT_BYTES: usize = 512 * 4;
const ERROR_EXCERPT_SCALARS: usize = 512;
// A sample is 30s of speech; anything past this is not a transcript.
const TRANSCRIPT_BYTES: usize = 64 * 1024;

pub struct WhisperLikeTranscriber {
    pub descriptor: &'static ProviderDescriptor,
    api_key: SecretString,
    http: Client,
    endpoint_override: Option<String>,
}

impl WhisperLikeTranscriber {
    pub fn new(
        descriptor: &'static ProviderDescriptor,
        api_key: SecretString,
        http: Client,
    ) -> Self {
        Self {
            descriptor,
            api_key,
            http,
            endpoint_override: None,
        }
    }

    /// Test seam. The bearer key must never travel over cleartext, so only
    /// https or loopback http endpoints are accepted.
    pub fn with_endpoint(
        descriptor: &'static ProviderDescriptor,
        api_key: SecretString,
        http: Client,
        endpoint: String,
    ) -> Self {
        assert!(
            is_confidential_endpoint(&endpoint),
            "endpoint override must be https or loopback http"
        );
        Self {
            descriptor,
            api_key,
            http,
            endpoint_override: Some(endpoint),
        }
    }

    fn endpoint(&self) -> &str {
        self.endpoint_override
            .as_deref()
            .unwrap_or(self.descriptor.endpoint)
    }
}

impl Transcriber for WhisperLikeTranscriber {
    fn provider_id(&self) -> TranscribeProviderId {
        self.descriptor.id
    }

    fn transcribe(
        &self,
        audio: &Path,
        opts: &TranscribeOpts,
    ) -> BoxFuture<'_, Result<Transcript, TranscribeError>> {
        let audio = audio.to_path_buf();
        let language = opts.language.clone();
        let prompt = opts.prompt.clone();

        Box::pin(async move {
            let mut form = Form::new()
                .file("file", audio)
                .await
                .map_err(|error| {
                    TranscribeError::new(
                        TranscribeErrorKind::Audio,
                        format!("unable to read audio file: {error}"),
                    )
                })?
                .text("model", self.descriptor.model)
                .text("response_format", "text");

            if let Some(language) = language {
                form = form.text("language", language);
            }
            if let Some(prompt) = prompt {
                form = form.text("prompt", prompt);
            }

            let request = self
                .http
                .post(self.endpoint())
                .bearer_auth(self.api_key.expose_secret())
                .multipart(form);

            tokio::time::timeout(REQUEST_TIMEOUT, async {
                let response = request.send().await.map_err(transport_error)?;

                if !response.status().is_success() {
                    let status = response.status();
                    let excerpt = error_excerpt(response).await;
                    let detail = if excerpt.is_empty() {
                        String::new()
                    } else {
                        format!(": {excerpt}")
                    };
                    return Err(TranscribeError::new(
                        status_error_kind(status),
                        format!("provider returned HTTP {}{detail}", status.as_u16()),
                    ));
                }

                if let Some(kind) = non_transcript_media_type(&response) {
                    return Err(TranscribeError::new(
                        kind,
                        "provider returned a non-transcript response",
                    ));
                }

                let body = try_capped_body(response, TRANSCRIPT_BYTES)
                    .await
                    .map_err(|(_, error)| transport_error(error))?;
                let text = String::from_utf8_lossy(&body);
                let text = text.trim();
                if looks_like_html(text) {
                    return Err(TranscribeError::new(
                        TranscribeErrorKind::ProviderFailed,
                        "provider returned a non-transcript response",
                    ));
                }
                Ok(Transcript {
                    text: text.to_owned(),
                })
            })
            .await
            .map_err(|_| timeout_error())?
        })
    }
}

fn is_confidential_endpoint(endpoint: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(endpoint) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        ),
        _ => false,
    }
}

/// A captive portal or proxy answers 200 with a login page; aligning that as a
/// transcript costs the user a second paid call on the same garbage.
fn non_transcript_media_type(response: &reqwest::Response) -> Option<TranscribeErrorKind> {
    let media_type = response.headers().get(CONTENT_TYPE)?.to_str().ok()?;
    let essence = media_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match essence.as_str() {
        "text/plain" | "application/json" => None,
        _ => Some(TranscribeErrorKind::ProviderFailed),
    }
}

fn looks_like_html(body: &str) -> bool {
    let head = body
        .chars()
        .take(20)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html")
}

fn transport_error(error: reqwest::Error) -> TranscribeError {
    if error.is_timeout() {
        return timeout_error();
    }

    tracing::warn!(error = %error, "transcription transport failure");
    let mut cause: &dyn std::error::Error = &error;
    while let Some(source) = cause.source() {
        cause = source;
    }
    TranscribeError::new(
        TranscribeErrorKind::Network,
        format!(
            "transcription network request failed ({}): {cause}",
            transport_stage(&error)
        ),
    )
}

fn transport_stage(error: &reqwest::Error) -> &'static str {
    if error.is_connect() {
        "connect"
    } else if error.is_body() || error.is_decode() {
        "response body"
    } else if error.is_request() {
        "request"
    } else {
        "transport"
    }
}

fn timeout_error() -> TranscribeError {
    TranscribeError::new(
        TranscribeErrorKind::Timeout,
        "transcription request timed out",
    )
}

fn status_error_kind(status: StatusCode) -> TranscribeErrorKind {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => TranscribeErrorKind::Unauthorized,
        StatusCode::TOO_MANY_REQUESTS => TranscribeErrorKind::RateLimit,
        _ => TranscribeErrorKind::ProviderFailed,
    }
}

/// Best-effort: a truncated or failed read still yields whatever arrived, which
/// is all an error excerpt needs.
async fn capped_body(response: reqwest::Response, limit: usize) -> Vec<u8> {
    try_capped_body(response, limit)
        .await
        .unwrap_or_else(|(bytes, _)| bytes)
}

/// Fallible twin for the transcript path, where a mid-body timeout must surface
/// as a transport error rather than an empty transcript.
async fn try_capped_body(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, (Vec<u8>, reqwest::Error)> {
    let mut bytes = Vec::new();
    let mut chunks = response.bytes_stream();

    while bytes.len() < limit {
        match chunks.next().await {
            Some(Ok(chunk)) => {
                let remaining = limit - bytes.len();
                bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            }
            Some(Err(error)) => return Err((bytes, error)),
            None => break,
        }
    }

    Ok(bytes)
}

async fn error_excerpt(response: reqwest::Response) -> String {
    let bytes = capped_body(response, ERROR_EXCERPT_BYTES).await;
    let excerpt: String = String::from_utf8_lossy(&bytes)
        .chars()
        .take(ERROR_EXCERPT_SCALARS)
        .collect();
    scrub_secrets(excerpt.trim())
        .chars()
        .take(ERROR_EXCERPT_SCALARS)
        .collect()
}

fn is_secret_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '+' | '/')
}

/// Provider error bodies reach the UI, and providers echo keys back in them
/// with arbitrary syntax around them (`Bearer sk-x`, `{"bearer":"sk-x"}`,
/// `Incorrect API key provided: sk-x`). Redact key-shaped tokens and whatever
/// follows a `bearer` word, whatever separates them.
fn scrub_secrets(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    let mut after_bearer = false;

    while !rest.is_empty() {
        let (separator, tail) = rest.split_at(rest.find(is_secret_char).unwrap_or(rest.len()));
        output.push_str(separator);
        if tail.is_empty() {
            break;
        }
        let (token, next) = tail.split_at(tail.find(|c| !is_secret_char(c)).unwrap_or(tail.len()));

        if token.eq_ignore_ascii_case("bearer") {
            output.push_str(token);
            after_bearer = true;
        } else if after_bearer || is_key_shaped(token) {
            output.push_str("[redacted]");
            after_bearer = false;
        } else {
            output.push_str(token);
        }
        rest = next;
    }

    output
}

fn is_key_shaped(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    ["sk-", "sk_", "gsk-", "gsk_"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}
