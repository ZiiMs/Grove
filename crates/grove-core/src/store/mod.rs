//! Module root for the SQLite store: one file, ordered migrations, no ORM (see `architecture.md`
//! §Persistence).

pub mod db;
pub mod repos;

pub use db::{Store, StoreError, MIGRATIONS};
