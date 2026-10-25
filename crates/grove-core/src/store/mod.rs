//! Module root for the SQLite store: one file, ordered migrations, no ORM (see `architecture.md`
//! §Persistence).

pub mod db;
pub mod pins;
pub mod plans;
pub mod projects;
pub mod repos;

pub use db::{Store, StoreError, MIGRATIONS};
pub use plans::PlanRow;
