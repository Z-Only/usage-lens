//! Explicit, bounded local adapters. No automatic discovery or collection.
pub mod collect;
pub mod demo;
pub mod hooks;
pub mod incremental;
pub mod launch;
pub mod read_only_rpc;
pub mod rollout;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct AdapterError(pub &'static str);

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
