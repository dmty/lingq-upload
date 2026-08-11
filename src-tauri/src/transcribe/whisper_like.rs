use std::path::Path;

use futures::future::BoxFuture;
use reqwest::multipart::Form;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use super::provider::ProviderDescriptor;
use super::{
    TranscribeError, TranscribeErrorKind, TranscribeOpts, TranscribeProviderId, Transcriber,
    Transcript,
};

pub(super) struct WhisperLikeTranscriber {
    descriptor: &'static ProviderDescriptor,
    key: SecretString,
    http_client: Client,
}

#[derive(Deserialize)]
struct WhisperResponse {
    text: String,
}

impl WhisperLikeTranscriber {
    pub(super) fn new(
        descriptor: &'static ProviderDescriptor,
        key: SecretString,
        http_client: Client,
    ) -> Self {
        Self {
            descriptor,
            key,
            http_client,
        }
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
                .map_err(|error| TranscribeError::new(TranscribeErrorKind::Io, error.to_string()))?
                .text("model", self.descriptor.model);

            if let Some(language) = language {
                form = form.text("language", language);
            }
            if let Some(prompt) = prompt {
                form = form.text("prompt", prompt);
            }

            let response = self
                .http_client
                .post(self.descriptor.endpoint)
                .bearer_auth(self.key.expose_secret())
                .multipart(form)
                .send()
                .await
                .map_err(|error| {
                    TranscribeError::new(TranscribeErrorKind::Transport, error.to_string())
                })?
                .error_for_status()
                .map_err(|error| {
                    TranscribeError::new(TranscribeErrorKind::Provider, error.to_string())
                })?;

            let response: WhisperResponse = response.json().await.map_err(|error| {
                TranscribeError::new(TranscribeErrorKind::InvalidResponse, error.to_string())
            })?;

            Ok(Transcript {
                text: response.text,
            })
        })
    }
}
