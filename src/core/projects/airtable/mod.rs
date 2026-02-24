pub mod client;
pub mod types;

pub use client::{fetch_bases, fetch_tables, parse_airtable_record_id, OptionalAirtableClient};
pub use types::{AirtableTaskStatus, StatusOption};
