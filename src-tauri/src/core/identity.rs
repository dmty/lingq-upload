use std::fmt::Write;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
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
    pub content_hash: [u8; 32],
    #[serde(default)]
    pub audible_asin: Option<String>,
    #[serde(default)]
    pub isbn13: Option<String>,
    #[serde(default)]
    pub calibre_uuid: Option<Uuid>,
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

    pub fn with_asin(mut self, asin: impl Into<String>) -> Self {
        self.audible_asin = Some(asin.into());
        self
    }

    pub fn with_isbn13(mut self, isbn13: impl Into<String>) -> Self {
        self.isbn13 = Some(isbn13.into());
        self
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
        format!("ch:{}", hex_encode(&self.content_hash))
    }

    /// Any-of-many strong-key match. Falls back to `content_hash` equality.
    /// `None` slots on either side are ignored.
    pub fn matches(&self, other: &Self) -> bool {
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

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(&mut s, "{b:02x}").unwrap();
    }
    s
}

mod hex_array_32 {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::hex_encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let raw = hex::decode(&s).map_err(D::Error::custom)?;
        let arr: [u8; 32] = raw
            .try_into()
            .map_err(|v: Vec<u8>| D::Error::custom(format!("expected 32 bytes, got {}", v.len())))?;
        Ok(arr)
    }
}
