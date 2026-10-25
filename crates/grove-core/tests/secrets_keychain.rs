//! The one test that touches the real OS keychain (Windows Credential Manager on the build target).
//!
//! It is `#[ignore]`d so the normal suite never prompts for, or writes to, the user's credential store.
//! Run it explicitly:
//!
//! ```text
//! cargo test -p grove-core --test secrets_keychain -- --ignored
//! ```

use grove_core::secrets::{KeychainSecretStore, SecretStatus, SecretStore};

#[test]
#[ignore = "writes to the real OS keychain; run with `-- --ignored`"]
fn the_real_keychain_round_trips_a_secret() {
    let store = KeychainSecretStore::grove();
    let key = "grove.test.keychain.round_trip";

    if store.status(key) == SecretStatus::Unavailable {
        // A locked session or a machine with no credential manager is a valid environment for an
        // opt-in test; `Unavailable` never panics and never masquerades as `Absent`.
        eprintln!("skipping: the OS keychain is unavailable in this session");
        return;
    }

    // Never put the value in a panic message or a log line; only a random test token is ever stored.
    let value = format!("grove-test-{}", uuid::Uuid::new_v4());
    let _ = store.delete(key);

    store
        .set(key, &value)
        .expect("set a secret in the keychain");
    assert_eq!(store.status(key), SecretStatus::Present);
    assert_eq!(store.get(key).unwrap().as_deref(), Some(value.as_str()));

    store.delete(key).expect("delete the secret");
    assert_eq!(store.status(key), SecretStatus::Absent);
    assert_eq!(store.get(key).unwrap(), None);

    // Deleting twice is idempotent, not an error.
    store.delete(key).expect("deleting a missing key succeeds");
}
