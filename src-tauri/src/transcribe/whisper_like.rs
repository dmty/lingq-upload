use std::path::Path;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::StreamExt;
use reqwest::multipart::Form;
use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};

use super::provider::ProviderDescriptor;
use super::{
    TranscribeError, TranscribeErrorKind, TranscribeOpts, TranscribeProviderId, Transcriber,
    Transcript,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const ERROR_EXCERPT_BYTES: usize = 512 * 4;
const ERROR_EXCERPT_SCALARS: usize = 512;

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

    pub fn with_endpoint(
        descriptor: &'static ProviderDescriptor,
        api_key: SecretString,
        http: Client,
        endpoint: String,
    ) -> Self {
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
                let response = tokio::time::timeout(CONNECT_TIMEOUT, request.send())
                    .await
                    .map_err(|_| timeout_error())?
                    .map_err(transport_error)?;

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

                let text = response.text().await.map_err(transport_error)?;
                Ok(Transcript {
                    text: text.trim().to_owned(),
                })
            })
            .await
            .map_err(|_| timeout_error())?
        })
    }
}

fn transport_error(error: reqwest::Error) -> TranscribeError {
    if error.is_timeout() {
        timeout_error()
    } else {
        TranscribeError::new(
            TranscribeErrorKind::Network,
            "transcription network request failed",
        )
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

async fn error_excerpt(response: reqwest::Response) -> String {
    let mut bytes = Vec::with_capacity(ERROR_EXCERPT_BYTES);
    let mut chunks = response.bytes_stream();

    while bytes.len() < ERROR_EXCERPT_BYTES {
        let Some(Ok(chunk)) = chunks.next().await else {
            break;
        };
        let remaining = ERROR_EXCERPT_BYTES - bytes.len();
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }

    let excerpt: String = String::from_utf8_lossy(&bytes)
        .chars()
        .take(ERROR_EXCERPT_SCALARS)
        .collect();
    scrub_bearer_values(excerpt.trim())
        .chars()
        .take(ERROR_EXCERPT_SCALARS)
        .collect()
}

fn scrub_bearer_values(input: &str) -> String {
    let bytes = input.as_bytes();
    let lower = input.to_ascii_lowercase();
    let lower = lower.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut copied_through = 0;
    let mut cursor = 0;

    while cursor + 6 <= bytes.len() {
        if &lower[cursor..cursor + 6] == b"bearer"
            && (cursor == 0 || !bytes[cursor - 1].is_ascii_alphanumeric())
        {
            let mut token_start = cursor + 6;
            while token_start < bytes.len() && bytes[token_start].is_ascii_whitespace() {
                token_start += 1;
            }
            if token_start > cursor + 6 && token_start < bytes.len() {
                let mut token_end = token_start;
                while token_end < bytes.len()
                    && !bytes[token_end].is_ascii_whitespace()
                    && !b",;}]".contains(&bytes[token_end])
                {
                    token_end += 1;
                }
                output.push_str(&input[copied_through..token_start]);
                output.push_str("[redacted]");
                copied_through = token_end;
                cursor = token_end;
                continue;
            }
        }
        cursor += 1;
    }

    output.push_str(&input[copied_through..]);
    output
}
