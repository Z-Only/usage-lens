//! Local-only SQLite storage and allowlisted JSON query contracts.
pub mod normalize;
pub mod redact;
pub mod response_tokens;
mod store;
pub mod validation;
pub use store::{UsageStore, trace};
pub use validation::{CoreError, CoreResult};
