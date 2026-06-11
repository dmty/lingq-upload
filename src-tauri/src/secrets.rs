use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

// Sourced from tauri.conf.json at build time (see build.rs) so the keychain
// prompt always shows the app's real bundle identity.
const SERVICE: &str = env!("LINGQ_BUNDLE_ID");
const ACCOUNT: &str = "lingq_api_key";

/// Which backend the secrets layer uses. In release builds the choice is
/// fixed to `Keychain`; the variant exists in the type so the IPC surface
/// stays the same across debug/release.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendChoice {
    File,
    Keychain,
}

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

    /// Construct using the platform-appropriate default backend, resolved
    /// from build profile, env override, and persisted dev prefs.
    pub fn new_default(app_data_dir: &Path) -> Self {
        Self::new(default_backend(app_data_dir))
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

/// Picks the backend for a `SecretsStore`. Release builds are pinned to the
/// OS keychain. Debug builds default to a file shim under `app_data_dir` so
/// that `cargo tauri dev` rebuilds do not retrigger the keychain ACL prompt
/// on every restart; this can be overridden via the `LINGQ_USE_REAL_KEYCHAIN`
/// env var or the persisted [`DevPrefs::backend`] choice.
pub fn default_backend(app_data_dir: &Path) -> Box<dyn KeyringBackend> {
    #[cfg(debug_assertions)]
    {
        if std::env::var("LINGQ_USE_REAL_KEYCHAIN").is_ok() {
            tracing::warn!("dev secrets: real keychain (LINGQ_USE_REAL_KEYCHAIN set)");
            return Box::new(RealKeyring::new());
        }
        let prefs = dev_prefs_load(app_data_dir);
        match prefs.backend.unwrap_or(BackendChoice::File) {
            BackendChoice::Keychain => {
                tracing::warn!("dev secrets: real keychain (per dev-prefs)");
                Box::new(RealKeyring::new())
            }
            BackendChoice::File => {
                let path = app_data_dir.join("dev-secrets.json");
                tracing::warn!(?path, "dev secrets: file shim active (debug build)");
                Box::new(FileBackend::new(path))
            }
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = app_data_dir;
        Box::new(RealKeyring::new())
    }
}

/// Dev-only file backend. Keeps secrets in a JSON file under `app_data_dir`
/// so the OS keychain ACL is not consulted on every rebuild.
#[cfg(debug_assertions)]
pub struct FileBackend {
    path: PathBuf,
}

#[cfg(debug_assertions)]
#[derive(Debug, Default, Serialize, Deserialize)]
struct FileStore {
    entries: Vec<FileEntry>,
}

#[cfg(debug_assertions)]
#[derive(Debug, Serialize, Deserialize)]
struct FileEntry {
    service: String,
    account: String,
    value: String,
}

#[cfg(debug_assertions)]
impl FileBackend {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> Result<FileStore, SecretError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| SecretError::Backend(format!("dev-secrets parse: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileStore::default()),
            Err(e) => Err(SecretError::Backend(format!("dev-secrets read: {e}"))),
        }
    }

    fn save(&self, store: &FileStore) -> Result<(), SecretError> {
        use std::io::Write;

        let parent = self
            .path
            .parent()
            .ok_or_else(|| SecretError::Backend("dev-secrets: no parent dir".into()))?;
        std::fs::create_dir_all(parent)
            .map_err(|e| SecretError::Backend(format!("dev-secrets mkdir: {e}")))?;
        let bytes = serde_json::to_vec_pretty(store)
            .map_err(|e| SecretError::Backend(format!("dev-secrets encode: {e}")))?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| SecretError::Backend(format!("dev-secrets tmp: {e}")))?;
        tmp.write_all(&bytes)
            .map_err(|e| SecretError::Backend(format!("dev-secrets write: {e}")))?;
        tmp.persist(&self.path)
            .map_err(|e| SecretError::Backend(format!("dev-secrets rename: {e}")))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

#[cfg(debug_assertions)]
impl KeyringBackend for FileBackend {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), SecretError> {
        let mut store = self.load()?;
        store
            .entries
            .retain(|e| !(e.service == service && e.account == account));
        store.entries.push(FileEntry {
            service: service.into(),
            account: account.into(),
            value: value.into(),
        });
        self.save(&store)
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<String>, SecretError> {
        let store = self.load()?;
        Ok(store
            .entries
            .into_iter()
            .find(|e| e.service == service && e.account == account)
            .map(|e| e.value))
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), SecretError> {
        let mut store = self.load()?;
        let before = store.entries.len();
        store
            .entries
            .retain(|e| !(e.service == service && e.account == account));
        if store.entries.len() == before {
            // Match RealKeyring: clearing a non-existent key is a no-op.
            return Ok(());
        }
        self.save(&store)
    }
}

/// Dev-only preferences persisted alongside `dev-secrets.json`. Used to let
/// developers opt into the real OS keychain from the settings UI without
/// rebuilding.
#[cfg(debug_assertions)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DevPrefs {
    #[serde(default)]
    pub backend: Option<BackendChoice>,
}

#[cfg(debug_assertions)]
const DEV_PREFS_FILE: &str = "dev-prefs.json";

#[cfg(debug_assertions)]
pub fn dev_prefs_load(app_data_dir: &Path) -> DevPrefs {
    let path = app_data_dir.join(DEV_PREFS_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => DevPrefs::default(),
    }
}

#[cfg(debug_assertions)]
pub fn dev_prefs_save(app_data_dir: &Path, prefs: &DevPrefs) -> std::io::Result<()> {
    std::fs::create_dir_all(app_data_dir)?;
    let path = app_data_dir.join(DEV_PREFS_FILE);
    let bytes = serde_json::to_vec_pretty(prefs)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, bytes)
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

    #[cfg(debug_assertions)]
    mod file_backend {
        use super::super::*;
        use tempfile::TempDir;

        fn backend(dir: &TempDir) -> FileBackend {
            FileBackend::new(dir.path().join("dev-secrets.json"))
        }

        #[test]
        fn round_trip_save_load_clear() {
            let dir = TempDir::new().expect("tmp");
            let be = backend(&dir);

            assert_eq!(be.get("svc", "acct").expect("get empty"), None);

            be.set("svc", "acct", "v1").expect("set");
            assert_eq!(be.get("svc", "acct").expect("get"), Some("v1".into()));

            be.set("svc", "acct", "v2").expect("overwrite");
            assert_eq!(be.get("svc", "acct").expect("get"), Some("v2".into()));

            be.delete("svc", "acct").expect("delete");
            assert_eq!(be.get("svc", "acct").expect("get after delete"), None);
        }

        #[test]
        fn delete_missing_is_noop() {
            let dir = TempDir::new().expect("tmp");
            let be = backend(&dir);
            be.delete("svc", "acct").expect("delete missing");
        }

        #[test]
        fn missing_file_loads_empty() {
            let dir = TempDir::new().expect("tmp");
            let be = backend(&dir);
            assert_eq!(be.get("svc", "acct").expect("get"), None);
            assert!(!dir.path().join("dev-secrets.json").exists());
        }

        #[test]
        fn malformed_file_lifts_to_backend_error() {
            let dir = TempDir::new().expect("tmp");
            let path = dir.path().join("dev-secrets.json");
            std::fs::write(&path, b"not json").expect("seed");
            let be = FileBackend::new(path);
            match be.get("svc", "acct").unwrap_err() {
                SecretError::Backend(_) => {}
                other => panic!("expected Backend error, got {other:?}"),
            }
        }

        #[cfg(unix)]
        #[test]
        fn file_permissions_are_0600() {
            use std::os::unix::fs::PermissionsExt;
            let dir = TempDir::new().expect("tmp");
            let be = backend(&dir);
            be.set("svc", "acct", "v").expect("set");
            let mode = std::fs::metadata(dir.path().join("dev-secrets.json"))
                .expect("stat")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        #[test]
        fn dev_prefs_round_trip() {
            let dir = TempDir::new().expect("tmp");
            let loaded_default = dev_prefs_load(dir.path());
            assert_eq!(loaded_default.backend, None);

            let prefs = DevPrefs {
                backend: Some(BackendChoice::Keychain),
            };
            dev_prefs_save(dir.path(), &prefs).expect("save");
            let loaded = dev_prefs_load(dir.path());
            assert_eq!(loaded.backend, Some(BackendChoice::Keychain));
        }

        #[test]
        fn dev_prefs_load_tolerates_malformed_file() {
            let dir = TempDir::new().expect("tmp");
            std::fs::write(dir.path().join(DEV_PREFS_FILE), b"garbage").expect("seed");
            let loaded = dev_prefs_load(dir.path());
            assert_eq!(loaded.backend, None);
        }
    }
}
