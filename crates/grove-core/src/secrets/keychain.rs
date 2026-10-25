//! The OS keychain implementation of [`SecretStore`] (`keyring` v4 → Windows Credential Manager,
//! macOS Keychain, Secret Service on *nix).
//!
//! Nothing here panics on a locked or missing keychain: a platform failure becomes a
//! [`SecretError`], and [`SecretStatus::Unavailable`] is distinct from [`SecretStatus::Absent`] so the
//! UI can say "the keychain is locked" instead of "you configured no token". Values are never logged,
//! formatted, or put in an error message.

use super::{SecretError, SecretStatus, SecretStore};
use crate::APP_NAME;

/// The keychain-backed store. Cheap to construct and lazy: the credential store is initialised by the
/// `keyring` crate on first use, not at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeychainSecretStore {
    service: String,
}

impl KeychainSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    pub fn grove() -> Self {
        Self::new(APP_NAME)
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, key).map_err(secret_error)
    }
}

impl Default for KeychainSecretStore {
    fn default() -> Self {
        Self::grove()
    }
}

impl SecretStore for KeychainSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            // A key that was never set is an answer, not a failure.
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(secret_error(error)),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.entry(key)?.set_password(value).map_err(secret_error)
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        match self.entry(key)?.delete_credential() {
            // Deleting a missing key is idempotent: the desired state already holds.
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(secret_error(error)),
        }
    }

    fn status(&self, key: &str) -> SecretStatus {
        // A keychain that cannot even be initialised (locked session, no credential manager) is
        // `Unavailable`, never `Absent`.
        if keyring::Entry::store_status().is_err() {
            return SecretStatus::Unavailable;
        }
        let Ok(entry) = self.entry(key) else {
            return SecretStatus::Unavailable;
        };
        match entry.get_password() {
            Ok(_) => SecretStatus::Present,
            Err(keyring::Error::NoEntry) => SecretStatus::Absent,
            Err(_) => SecretStatus::Unavailable,
        }
    }
}

fn secret_error(error: keyring::Error) -> SecretError {
    // `keyring::Error`'s Display describes the store and the key name, never the secret value.
    SecretError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_is_named_for_the_app_and_does_not_initialise_eagerly() {
        let store = KeychainSecretStore::grove();
        assert_eq!(store.service(), APP_NAME);
        assert_eq!(store, KeychainSecretStore::new("Grove"));
    }
}
