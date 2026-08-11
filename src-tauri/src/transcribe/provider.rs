use reqwest::Client;
use secrecy::SecretString;
use serde::Serialize;
use specta::Type;

use super::{
    TranscribeError, TranscribeErrorKind, TranscribeProviderId, Transcriber, WhisperLikeTranscriber,
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
pub struct ProviderDescriptor {
    pub id: TranscribeProviderId,
    pub label: &'static str,
    pub model: &'static str,
    pub endpoint: &'static str,
    pub pricing: PricingHint,
    pub data_policy_url: &'static str,
    pub supported_languages: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
pub struct PricingHint {
    pub summary: &'static str,
    pub estimated_usd_per_minute: Option<f64>,
    pub free_tier_eligible: bool,
    pub docs_url: &'static str,
}

static BUILT_IN_PROVIDERS: [ProviderDescriptor; 2] = [
    ProviderDescriptor {
        id: TranscribeProviderId::Groq,
        label: "Groq",
        model: "whisper-large-v3-turbo",
        endpoint: "https://api.groq.com/openai/v1/audio/transcriptions",
        pricing: PricingHint {
            summary:
                "Free-tier eligible; limits depend on your account/tier; current paid reference $0.04/hour",
            estimated_usd_per_minute: Some(0.000_666_666_666_666_666_6),
            free_tier_eligible: true,
            docs_url: "https://console.groq.com/docs/speech-to-text",
        },
        data_policy_url: "https://console.groq.com/docs/your-data",
        supported_languages: &[],
    },
    ProviderDescriptor {
        id: TranscribeProviderId::OpenAi,
        label: "OpenAI",
        model: "whisper-1",
        endpoint: "https://api.openai.com/v1/audio/transcriptions",
        pricing: PricingHint {
            summary: "No free tier; current reference $0.006/min",
            estimated_usd_per_minute: Some(0.006),
            free_tier_eligible: false,
            docs_url: "https://developers.openai.com/api/docs/models/whisper-1",
        },
        data_policy_url:
            "https://platform.openai.com/docs/models/default-usage-policies-by-endpoint",
        supported_languages: &[],
    },
];

pub struct ProviderCatalog;

impl ProviderCatalog {
    pub fn built_in() -> Self {
        Self
    }

    pub fn descriptors(&self) -> &'static [ProviderDescriptor] {
        &BUILT_IN_PROVIDERS
    }

    pub fn descriptor(
        &self,
        id: TranscribeProviderId,
    ) -> Result<&'static ProviderDescriptor, TranscribeError> {
        self.descriptors()
            .iter()
            .find(|descriptor| descriptor.id == id)
            .ok_or_else(|| {
                TranscribeError::new(
                    TranscribeErrorKind::ProviderFailed,
                    format!("transcription provider {id:?} is not registered"),
                )
            })
    }

    pub fn create(
        &self,
        provider_id: TranscribeProviderId,
        key: SecretString,
        http_client: Client,
    ) -> Result<Box<dyn Transcriber>, TranscribeError> {
        Ok(Box::new(WhisperLikeTranscriber::new(
            self.descriptor(provider_id)?,
            key,
            http_client,
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use futures::future::BoxFuture;
    use secrecy::SecretString;

    use super::ProviderCatalog;
    use crate::transcribe::{
        TranscribeError, TranscribeErrorKind, TranscribeOpts, TranscribeProviderId, Transcriber,
        Transcript,
    };

    struct FakeTranscriber;

    impl Transcriber for FakeTranscriber {
        fn provider_id(&self) -> TranscribeProviderId {
            TranscribeProviderId::Groq
        }

        fn transcribe(
            &self,
            _: &Path,
            _: &TranscribeOpts,
        ) -> BoxFuture<'_, Result<Transcript, TranscribeError>> {
            Box::pin(async {
                Ok(Transcript {
                    text: "fixture".into(),
                })
            })
        }
    }

    fn accepts_fake(_: Box<dyn Transcriber>) {}

    #[test]
    fn built_in_catalog_is_fixed_keyless_and_stable() {
        let catalog = ProviderCatalog::built_in();
        let ids: Vec<_> = catalog
            .descriptors()
            .iter()
            .map(|descriptor| descriptor.id)
            .collect();

        assert_eq!(
            ids,
            [TranscribeProviderId::Groq, TranscribeProviderId::OpenAi]
        );
        accepts_fake(Box::new(FakeTranscriber));
    }

    #[test]
    fn built_in_catalog_metadata_is_exact() {
        let catalog = ProviderCatalog::built_in();
        let groq = catalog.descriptor(TranscribeProviderId::Groq).unwrap();
        let open_ai = catalog.descriptor(TranscribeProviderId::OpenAi).unwrap();

        assert_eq!(groq.label, "Groq");
        assert_eq!(groq.model, "whisper-large-v3-turbo");
        assert_eq!(
            groq.endpoint,
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        assert_eq!(
            groq.pricing.summary,
            "Free-tier eligible; limits depend on your account/tier; current paid reference $0.04/hour"
        );
        assert_eq!(
            groq.pricing.estimated_usd_per_minute,
            Some(0.000_666_666_666_666_666_6)
        );
        assert!(groq.pricing.free_tier_eligible);
        assert_eq!(
            groq.pricing.docs_url,
            "https://console.groq.com/docs/speech-to-text"
        );
        assert_eq!(
            groq.data_policy_url,
            "https://console.groq.com/docs/your-data"
        );

        assert_eq!(open_ai.label, "OpenAI");
        assert_eq!(open_ai.model, "whisper-1");
        assert_eq!(
            open_ai.endpoint,
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            open_ai.pricing.summary,
            "No free tier; current reference $0.006/min"
        );
        assert_eq!(open_ai.pricing.estimated_usd_per_minute, Some(0.006));
        assert!(!open_ai.pricing.free_tier_eligible);
        assert_eq!(
            open_ai.pricing.docs_url,
            "https://developers.openai.com/api/docs/models/whisper-1"
        );
        assert_eq!(
            open_ai.data_policy_url,
            "https://platform.openai.com/docs/models/default-usage-policies-by-endpoint"
        );
    }

    #[test]
    fn default_provider_is_groq() {
        assert_eq!(TranscribeProviderId::default(), TranscribeProviderId::Groq);
    }

    #[test]
    fn catalog_creates_each_shared_client_without_network_io() {
        let catalog = ProviderCatalog::built_in();

        for provider_id in [TranscribeProviderId::Groq, TranscribeProviderId::OpenAi] {
            let transcriber = catalog
                .create(
                    provider_id,
                    SecretString::from("fixture-key".to_owned()),
                    reqwest::Client::new(),
                )
                .unwrap();

            assert_eq!(transcriber.provider_id(), provider_id);
        }
    }

    #[tokio::test]
    async fn transcribe_reports_an_unreadable_audio_file_without_network_io() {
        let transcriber = ProviderCatalog::built_in()
            .create(
                TranscribeProviderId::Groq,
                SecretString::from("fixture-key".to_owned()),
                reqwest::Client::new(),
            )
            .unwrap();
        let opts = TranscribeOpts {
            language: None,
            prompt: None,
        };

        let error = transcriber
            .transcribe(Path::new("definitely-not-a-real-audio-file.mp3"), &opts)
            .await
            .unwrap_err();

        assert_eq!(error.kind(), TranscribeErrorKind::Audio);
    }
}
