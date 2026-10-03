#![recursion_limit = "256"]
//! Local-only Leptos dashboard. All domain state and request arbitration are platform-neutral.
#[cfg(all(feature = "csr", target_arch = "wasm32"))]
mod browser;
pub mod event_bridge;
pub mod model;
pub mod reading;
pub mod token_period;
pub mod views;
