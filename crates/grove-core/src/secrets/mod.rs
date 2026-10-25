//! Secret access.
//!
//! Secrets never live in `settings` or the database: the durable store only ever holds a key *name*.
//! [`KeychainSecretStore`] is the production implementation (ticket 07); [`MemorySecretStore`] keeps
//! tests and previews off the OS keychain. The rest of the app only ever sees [`SecretStore`], so it
//! does not care which one it was handed.

mod keychain;

pub use keychain::KeychainSecretStore;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

/// Key under which the optional GitHub token is stored. Never a value — only this name is durable.
pub const GITHUB_TOKEN: &str = "github.token";

/// Whether a secret can be read right now. `Unavailable` is distinct from `Absent`: a locked or
/// missing keychain must not look like "the user configured no token".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretStatus {
    Present,
    Absent,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("secret store error: {0}")]
pub struct SecretError(pub String);

impl SecretError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// The only way code reads or writes a secret. Implementations must never log values.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
    fn status(&self, key: &str) -> SecretStatus;
}

/// In-memory store for tests and for the pre-keychain core. `Debug` prints key names, never values.
#[derive(Default)]
pub struct MemorySecretStore {
    values: Mutex<BTreeMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        self.values
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self.lock().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.lock().insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.lock().remove(key);
        Ok(())
    }

    fn status(&self, key: &str) -> SecretStatus {
        if self.lock().contains_key(key) {
            SecretStatus::Present
        } else {
            SecretStatus::Absent
        }
    }
}

impl fmt::Debug for MemorySecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<String> = self.lock().keys().cloned().collect();
        f.debug_struct("MemorySecretStore")
            .field("keys", &keys)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_delete_round_trip() {
        let store = MemorySecretStore::new();
        assert_eq!(store.status(GITHUB_TOKEN), SecretStatus::Absent);
        store.set(GITHUB_TOKEN, "token").unwrap();
        assert_eq!(store.get(GITHUB_TOKEN).unwrap().as_deref(), Some("token"));
        assert_eq!(store.status(GITHUB_TOKEN), SecretStatus::Present);
        store.delete(GITHUB_TOKEN).unwrap();
        assert_eq!(store.get(GITHUB_TOKEN).unwrap(), None);
    }

    #[test]
    fn debug_redacts_values() {
        let store = MemorySecretStore::new();
        store.set(GITHUB_TOKEN, "ghp_never_print_me").unwrap();
        let debug = format!("{store:?}");
        assert!(debug.contains(GITHUB_TOKEN));
        assert!(!debug.contains("ghp_never_print_me"));
    }
}
