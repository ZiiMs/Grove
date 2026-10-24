//! The IPC wire types. Rust owns the shape; `packages/contract` mirrors it in zod and both sides are
//! pinned by the golden JSON in `contract/fixtures/`.

pub mod error;
pub mod types;

pub use error::{ErrorCode, IpcError, IpcResult};
pub use types::{
    AppInfo, CreateSessionRequest, EventEnvelope, HarnessInfo, Session, SessionDetail,
    SessionSummary,
};
