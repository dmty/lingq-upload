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

/// ISO 639-1 alpha-2 codes (sorted) for safe provider language hints.
const ISO_639_1: &[&str] = &[
    "aa", "ab", "ae", "af", "ak", "am", "an", "ar", "as", "av", "ay", "az", "ba", "be", "bg", "bh",
    "bi", "bm", "bn", "bo", "br", "bs", "ca", "ce", "ch", "co", "cr", "cs", "cu", "cv", "cy", "da",
    "de", "dv", "dz", "ee", "el", "en", "eo", "es", "et", "eu", "fa", "ff", "fi", "fj", "fo", "fr",
    "fy", "ga", "gd", "gl", "gn", "gu", "gv", "ha", "he", "hi", "ho", "hr", "ht", "hu", "hy", "hz",
    "ia", "id", "ie", "ig", "ii", "ik", "io", "is", "it", "iu", "ja", "jv", "ka", "kg", "ki", "kj",
    "kk", "kl", "km", "kn", "ko", "kr", "ks", "ku", "kv", "kw", "ky", "la", "lb", "lg", "li", "ln",
    "lo", "lt", "lu", "lv", "mg", "mh", "mi", "mk", "ml", "mn", "mr", "ms", "mt", "my", "na", "nb",
    "nd", "ne", "ng", "nl", "nn", "no", "nr", "nv", "ny", "oc", "oj", "om", "or", "os", "pa", "pi",
    "pl", "ps", "pt", "qu", "rm", "rn", "ro", "ru", "rw", "sa", "sc", "sd", "se", "sg", "si", "sk",
    "sl", "sm", "sn", "so", "sq", "sr", "ss", "st", "su", "sv", "sw", "ta", "te", "tg", "th", "ti",
    "tk", "tl", "tn", "to", "tr", "ts", "tt", "tw", "ty", "ug", "uk", "ur", "uz", "ve", "vi", "vo",
    "wa", "wo", "xh", "yi", "yo", "za", "zh", "zu",
];

fn is_iso_639_1(code: &str) -> bool {
    ISO_639_1.binary_search(&code).is_ok()
}

/// Map a LingQ project language to a provider ISO-639-1 hint when safe.
pub fn provider_language_hint(
    project_language: &str,
    descriptor: &ProviderDescriptor,
) -> Option<String> {
    let mapped = match project_language {
        "eng" => "en",
        "jpn" => "ja",
        "rus" => "ru",
        "spa" => "es",
        "deu" => "de",
        code if code.len() == 2 && code.chars().all(|c| c.is_ascii_lowercase()) => code,
        _ => return None,
    };
    if !is_iso_639_1(mapped) {
        return None;
    }
    if !descriptor.supported_languages.is_empty()
        && !descriptor.supported_languages.contains(&mapped)
    {
        return None;
    }
    Some(mapped.to_string())
}

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

    use super::{provider_language_hint, ProviderCatalog};
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

    #[test]
    fn provider_language_hint_maps_safe_codes() {
        let d = ProviderCatalog::built_in()
            .descriptor(TranscribeProviderId::Groq)
            .unwrap();
        assert_eq!(provider_language_hint("en", d).as_deref(), Some("en"));
        assert_eq!(provider_language_hint("ja", d).as_deref(), Some("ja"));
        assert_eq!(provider_language_hint("eng", d).as_deref(), Some("en"));
        assert_eq!(provider_language_hint("jpn", d).as_deref(), Some("ja"));
        assert_eq!(provider_language_hint("rus", d).as_deref(), Some("ru"));
        assert_eq!(provider_language_hint("spa", d).as_deref(), Some("es"));
        assert_eq!(provider_language_hint("deu", d).as_deref(), Some("de"));
        assert_eq!(provider_language_hint("EN", d), None);
        assert_eq!(provider_language_hint("xyz", d), None);
        assert_eq!(provider_language_hint("eng-", d), None);
        assert_eq!(provider_language_hint("", d), None);
        assert_eq!(provider_language_hint("e", d), None);
        // Non-ISO two-letter codes must not pass when supported_languages is empty.
        assert_eq!(provider_language_hint("zz", d), None);
    }
}
