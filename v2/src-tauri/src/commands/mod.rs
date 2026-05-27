//! Tauri command handlers — the thin bridge between the frontend and the
//! engine + ADB driver. Per architectural commitment #1, no business logic
//! lives here; commands fetch facts via the ADB driver, hand them to the
//! engine for decision-making, and return the result.

pub mod devices;
pub mod health;
pub mod launcher;
pub mod loader;
pub mod snapshot;
pub mod state;

pub use state::AppState;
