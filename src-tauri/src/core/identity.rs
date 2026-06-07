use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

/// Canonical project identity (AD-021).
///
/// `content_hash` is the always-present fallback. Strong keys
/// (`audible_asin` / `isbn13` / `calibre_uuid`) supply higher-confidence
/// joins when available across sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ProjectId {
    #[serde(with = "hex_array_32")]
    #[specta(type = String)]
    pub content_hash: [u8; 32],
    #[serde(default)]
    pub audible_asin: Option<String>,
    #[serde(default)]
    pub isbn13: Option<String>,
    #[serde(default)]
    pub calibre_uuid: Option<Uuid>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("invalid isbn13: {0}")]
    InvalidIsbn13(String),
    #[error("invalid asin: {0}")]
    InvalidAsin(String),
}

impl ProjectId {
    pub fn from_title_author(title: &str, author: &str) -> Self {
        Self {
            content_hash: content_hash(title, author),
            audible_asin: None,
            isbn13: None,
            calibre_uuid: None,
        }
    }

    /// Set Audible ASIN. Trims + uppercases; logs a warning when the
    /// normalised value does not match `^B0[A-Z0-9]{8}$` but stores it
    /// anyway (some legacy / regional catalogues issue off-shape IDs).
    pub fn with_asin(mut self, asin: impl Into<String>) -> Self {
        let raw = asin.into();
        let normalised = raw.trim().to_ascii_uppercase();
        if !is_well_formed_asin(&normalised) {
            tracing::warn!(asin = %normalised, "asin does not match B0[A-Z0-9]{{8}}");
        }
        self.audible_asin = Some(normalised);
        self
    }

    /// Set ISBN-13. Silently drops the value (with warning) if it is not
    /// exactly 13 ASCII digits — invalid ISBNs in a join slot are worse than
    /// no value at all. Use [`Self::try_with_isbn13`] for a fallible variant.
    pub fn with_isbn13(mut self, isbn13: impl Into<String>) -> Self {
        let raw = isbn13.into();
        match validate_isbn13(&raw) {
            Ok(clean) => self.isbn13 = Some(clean),
            Err(_) => {
                tracing::warn!(isbn13 = %raw, "isbn13 is not 13 ascii digits; dropping");
            }
        }
        self
    }

    pub fn try_with_isbn13(mut self, isbn13: impl Into<String>) -> Result<Self, IdentityError> {
        let raw = isbn13.into();
        self.isbn13 = Some(validate_isbn13(&raw)?);
        Ok(self)
    }

    pub fn with_calibre_uuid(mut self, uuid: Uuid) -> Self {
        self.calibre_uuid = Some(uuid);
        self
    }

    /// Stable join key. Precedence: asin > isbn13 > uuid > hex(content_hash).
    pub fn join_key(&self) -> String {
        if let Some(asin) = &self.audible_asin {
            return format!("asin:{asin}");
        }
        if let Some(isbn) = &self.isbn13 {
            return format!("isbn13:{isbn}");
        }
        if let Some(uuid) = &self.calibre_uuid {
            return format!("uuid:{uuid}");
        }
        format!("ch:{}", hex::encode(self.content_hash))
    }

    /// Two IDs match when their `join_key()`s are equal, OR when any strong-key
    /// slot present on both sides agrees, OR when the `content_hash` matches as
    /// a last resort.
    ///
    /// This is intentionally "any-of-many": we accept the trade-off that
    /// `matches` is not strictly transitive (A↔B by asin, B↔C by isbn does not
    /// imply A↔C). `join_key()` precedence (asin > isbn13 > uuid) is the
    /// single-key resolution used elsewhere; that key wins for grouping
    /// purposes when multiple strong keys are present and disagree.
    pub fn matches(&self, other: &Self) -> bool {
        if self.join_key() == other.join_key() {
            return true;
        }
        if let (Some(a), Some(b)) = (&self.audible_asin, &other.audible_asin) {
            if a == b {
                return true;
            }
        }
        if let (Some(a), Some(b)) = (&self.isbn13, &other.isbn13) {
            if a == b {
                return true;
            }
        }
        if let (Some(a), Some(b)) = (&self.calibre_uuid, &other.calibre_uuid) {
            if a == b {
                return true;
            }
        }
        self.content_hash == other.content_hash
    }
}

fn is_well_formed_asin(s: &str) -> bool {
    s.len() == 10
        && s.starts_with("B0")
        && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

fn validate_isbn13(s: &str) -> Result<String, IdentityError> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if cleaned.len() == 13 && cleaned.bytes().all(|b| b.is_ascii_digit()) {
        Ok(cleaned)
    } else {
        Err(IdentityError::InvalidIsbn13(s.to_string()))
    }
}

/// SHA-256 over `NFC(normalised_title) + "\x1f" + NFC(normalised_first_author)`.
/// Whitespace collapsed and lower-cased before hashing.
pub fn content_hash(title: &str, author: &str) -> [u8; 32] {
    let t = normalise(title);
    let a = normalise(author);
    let mut h = Sha256::new();
    h.update(t.as_bytes());
    h.update(b"\x1f");
    h.update(a.as_bytes());
    h.finalize().into()
}

fn normalise(s: &str) -> String {
    let nfc: String = s.nfc().collect();
    nfc.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

mod hex_array_32 {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let raw = hex::decode(s.to_ascii_lowercase()).map_err(D::Error::custom)?;
        let arr: [u8; 32] = raw
            .try_into()
            .map_err(|v: Vec<u8>| D::Error::custom(format!("expected 32 bytes, got {}", v.len())))?;
        Ok(arr)
    }
}
