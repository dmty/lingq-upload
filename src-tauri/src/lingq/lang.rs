use std::fmt;

use serde::{Deserialize, Serialize};

use super::error::LingqError;

/// Typed LingQ language code. Validated against `^[a-z]{2,3}$` — matches the
/// URL segment LingQ uses as a tenant boundary (see AD-017).
///
/// # Examples
///
/// ```
/// use lingq_upload_lib::lingq::LanguageCode;
/// assert!(LanguageCode::new("ja").is_ok());
/// assert!(LanguageCode::new("JA").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LanguageCode(String);

impl LanguageCode {
    pub fn new(code: &str) -> Result<Self, LingqError> {
        if Self::is_valid(code) {
            Ok(Self(code.to_string()))
        } else {
            Err(LingqError::Schema(format!(
                "invalid LingQ language code {code:?}; expected 2-3 lowercase ASCII letters"
            )))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_valid(code: &str) -> bool {
        let len = code.len();
        (2..=3).contains(&len) && code.bytes().all(|b| b.is_ascii_lowercase())
    }
}

impl TryFrom<String> for LanguageCode {
    type Error = LingqError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<LanguageCode> for String {
    fn from(code: LanguageCode) -> Self {
        code.0
    }
}

impl fmt::Display for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for LanguageCode {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_two_and_three_letter_lowercase() {
        for s in ["ja", "en", "ko", "ru", "fr", "zhs", "yue"] {
            assert!(LanguageCode::new(s).is_ok(), "{s} should be valid");
        }
    }

    #[test]
    fn rejects_uppercase_digits_and_wrong_length() {
        for s in ["", "j", "JA", "ja1", "Ja", "japa", "j-a", "ja "] {
            assert!(LanguageCode::new(s).is_err(), "{s:?} should be invalid");
        }
    }

    #[test]
    fn as_str_round_trip() {
        let c = LanguageCode::new("ja").expect("valid");
        assert_eq!(c.as_str(), "ja");
        assert_eq!(c.to_string(), "ja");
    }

    #[test]
    fn deserialize_validates_against_schema() {
        let ok: LanguageCode = serde_json::from_str("\"ja\"").expect("valid lowercase");
        assert_eq!(ok.as_str(), "ja");
        assert!(serde_json::from_str::<LanguageCode>("\"JA\"").is_err());
        assert!(serde_json::from_str::<LanguageCode>("\"japa\"").is_err());
        assert!(serde_json::from_str::<LanguageCode>("\"\"").is_err());
    }

    #[test]
    fn serialize_emits_bare_string() {
        let c = LanguageCode::new("ko").expect("valid");
        assert_eq!(serde_json::to_string(&c).expect("serialize"), "\"ko\"");
    }
}
