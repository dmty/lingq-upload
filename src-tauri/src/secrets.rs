use serde::Serialize;
use specta::Type;
use thiserror::Error;

// Match tauri.conf.json bundle identifier so macOS displays the app's real
// identity in the keychain prompt instead of a stale namespace.
const SERVICE: &str = "com.lingq.upload";
const ACCOUNT: &str = "lingq_api_key";

/// Errors raised by the secrets layer, lifted from the keyring backend.
///
/// Variants are intentionally coarse: the UI surfaces them as distinct
/// messages and `Backend(_)` is the catch-all so platform churn doesn't
/// break the contract.
#[derive(Error, Debug, Serialize, Type, Clone, PartialEq)]
#[serde(tag = "kind", content = "message")]
pub enum SecretError {
    #[error("keychain is locked")]
    LockedKeychain,
    #[error("user denied keychain access")]
    UserDenied,
    #[error("entry not found")]
    MissingEntry,
    #[error("keychain backend error: {0}")]
    Backend(String),
}

/// Indirection so unit tests can stub the OS keychain. The real impl
/// wraps the `keyring` crate; tests use an in-memory fake.
pub trait KeyringBackend: Send + Sync {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), SecretError>;
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, SecretError>;
    fn delete(&self, service: &str, account: &str) -> Result<(), SecretError>;
}

pub struct RealKeyring;

impl RealKeyring {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RealKeyring {
    fn default() -> Self {
        Self::new()
    }
}

/// Map keyring errors to our enum. macOS surfaces "user denied" as
/// `NoStorageAccess` with errSecAuthFailed (-25293) or errSecUserCanceled
/// (-128); the platform-specific code is buried in the inner error string,
/// so we sniff it. Anything we can't classify lumps into `Backend`.
fn map_keyring_err(e: keyring::Error) -> SecretError {
    match e {
        keyring::Error::NoEntry => SecretError::MissingEntry,
        keyring::Error::NoStorageAccess(inner) => classify_storage_access(&inner.to_string()),
        other => SecretError::Backend(other.to_string()),
    }
}

fn classify_storage_access(detail: &str) -> SecretError {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("denied")
        || lower.contains("user canceled")
        || lower.contains("user cancelled")
        || lower.contains("-128")
        || lower.contains("-25293")
    {
        SecretError::UserDenied
    } else if lower.contains("locked") || lower.contains("-25296") {
        SecretError::LockedKeychain
    } else {
        SecretError::Backend(detail.to_string())
    }
}

impl KeyringBackend for RealKeyring {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), SecretError> {
        let entry = keyring::Entry::new(service, account).map_err(map_keyring_err)?;
        entry.set_password(value).map_err(map_keyring_err)
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<String>, SecretError> {
        let entry = keyring::Entry::new(service, account).map_err(map_keyring_err)?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_err(e)),
        }
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), SecretError> {
        let entry = keyring::Entry::new(service, account).map_err(map_keyring_err)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Clearing a non-existent key is a no-op from the caller's POV.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring_err(e)),
        }
    }
}

pub struct SecretsStore {
    backend: Box<dyn KeyringBackend>,
}

impl SecretsStore {
    pub fn new(backend: Box<dyn KeyringBackend>) -> Self {
        Self { backend }
    }

    pub fn save_key(&self, key: &str) -> Result<(), SecretError> {
        self.backend.set(SERVICE, ACCOUNT, key)
    }

    pub fn load_key(&self) -> Result<Option<String>, SecretError> {
        self.backend.get(SERVICE, ACCOUNT)
    }

    pub fn clear_key(&self) -> Result<(), SecretError> {
        self.backend.delete(SERVICE, ACCOUNT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBackend {
        store: Mutex<HashMap<(String, String), String>>,
    }

    impl KeyringBackend for FakeBackend {
        fn set(&self, service: &str, account: &str, value: &str) -> Result<(), SecretError> {
            self.store
                .lock()
                .expect("fake store lock")
                .insert((service.into(), account.into()), value.into());
            Ok(())
        }

        fn get(&self, service: &str, account: &str) -> Result<Option<String>, SecretError> {
            Ok(self
                .store
                .lock()
                .expect("fake store lock")
                .get(&(service.into(), account.into()))
                .cloned())
        }

        fn delete(&self, service: &str, account: &str) -> Result<(), SecretError> {
            self.store
                .lock()
                .expect("fake store lock")
                .remove(&(service.into(), account.into()));
            Ok(())
        }
    }

    struct FailingBackend {
        err: SecretError,
    }

    impl KeyringBackend for FailingBackend {
        fn set(&self, _: &str, _: &str, _: &str) -> Result<(), SecretError> {
            Err(self.err.clone())
        }
        fn get(&self, _: &str, _: &str) -> Result<Option<String>, SecretError> {
            Err(self.err.clone())
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), SecretError> {
            Err(self.err.clone())
        }
    }

    #[test]
    fn round_trip_save_load_clear() {
        let store = SecretsStore::new(Box::new(FakeBackend::default()));

        assert_eq!(store.load_key().expect("load empty"), None);

        store.save_key("super-secret").expect("save");
        assert_eq!(
            store.load_key().expect("load after save"),
            Some("super-secret".into())
        );

        store.clear_key().expect("clear");
        assert_eq!(store.load_key().expect("load after clear"), None);
    }

    #[test]
    fn save_overwrites_existing() {
        let store = SecretsStore::new(Box::new(FakeBackend::default()));
        store.save_key("first").expect("save first");
        store.save_key("second").expect("save second");
        assert_eq!(store.load_key().expect("load"), Some("second".into()));
    }

    #[test]
    fn locked_keychain_lifts_to_distinct_variant() {
        let store = SecretsStore::new(Box::new(FailingBackend {
            err: SecretError::LockedKeychain,
        }));
        assert_eq!(
            store.save_key("k").unwrap_err(),
            SecretError::LockedKeychain
        );
        assert_eq!(store.load_key().unwrap_err(), SecretError::LockedKeychain);
        assert_eq!(store.clear_key().unwrap_err(), SecretError::LockedKeychain);
    }

    #[test]
    fn user_denied_lifts_to_distinct_variant() {
        let store = SecretsStore::new(Box::new(FailingBackend {
            err: SecretError::UserDenied,
        }));
        assert_eq!(store.save_key("k").unwrap_err(), SecretError::UserDenied);
        assert_eq!(store.load_key().unwrap_err(), SecretError::UserDenied);
        assert_eq!(store.clear_key().unwrap_err(), SecretError::UserDenied);
    }

    #[test]
    fn missing_entry_lifts_to_distinct_variant() {
        let store = SecretsStore::new(Box::new(FailingBackend {
            err: SecretError::MissingEntry,
        }));
        assert_eq!(store.save_key("k").unwrap_err(), SecretError::MissingEntry);
        assert_eq!(store.load_key().unwrap_err(), SecretError::MissingEntry);
        assert_eq!(store.clear_key().unwrap_err(), SecretError::MissingEntry);
    }

    #[test]
    fn classify_storage_access_detects_user_denied() {
        assert_eq!(
            classify_storage_access("User denied access"),
            SecretError::UserDenied
        );
        assert_eq!(
            classify_storage_access("errSecUserCanceled (-128)"),
            SecretError::UserDenied
        );
    }

    #[test]
    fn classify_storage_access_detects_locked() {
        assert_eq!(
            classify_storage_access("keychain is locked"),
            SecretError::LockedKeychain
        );
    }

    #[test]
    fn classify_storage_access_falls_back_to_backend() {
        match classify_storage_access("dbus pipe broke") {
            SecretError::Backend(msg) => assert!(msg.contains("dbus")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }
}
