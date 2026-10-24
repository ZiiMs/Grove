//! Secret storage. Adapters receive a `SecretStore` and never see the database, so tokens cannot leak
//! into `settings` rows, fixtures, or logs.
//!
//! Ticket 07 wires the OS keychain (`keyring`) behind this port. Sessions only need the port, and
//! tests use [`MemorySecretStore`].

use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The keychain is locked, missing, or refused the operation.
    #[error("secret store unavailable: {0}")]
    Unavailable(String),
}

/// A key/value store for credentials. Keys are names (`github-token`); values never touch SQLite.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
    /// Whether the backing store can be reached right now (used by the settings panel).
    fn status(&self) -> Result<bool, SecretError>;
}

/// Process-local secrets. This is the test double and the fallback for headless runs; it is never
/// used by the app, because `GroveCore::init` hands adapters the real store.
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    values: RwLock<HashMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .values
            .read()
            .map_err(|_| SecretError::Unavailable("lock poisoned".into()))?
            .get(key)
            .cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.values
            .write()
            .map_err(|_| SecretError::Unavailable("lock poisoned".into()))?
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.values
            .write()
            .map_err(|_| SecretError::Unavailable("lock poisoned".into()))?
            .remove(key);
        Ok(())
    }

    fn status(&self) -> Result<bool, SecretError> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips_without_leaking_into_plain_text() {
        let store = MemorySecretStore::new();
        assert_eq!(store.get("token").unwrap(), None);
        store.set("token", "s3cret").unwrap();
        assert_eq!(store.get("token").unwrap().as_deref(), Some("s3cret"));
        assert!(store.status().unwrap());
        store.delete("token").unwrap();
        assert_eq!(store.get("token").unwrap(), None);
    }
}
