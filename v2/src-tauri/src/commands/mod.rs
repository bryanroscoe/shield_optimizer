//! Tauri command handlers — the thin bridge between the frontend and the
//! engine + ADB driver. Per architectural commitment #1, no business logic
//! lives here; commands fetch facts via the ADB driver, hand them to the
//! engine for decision-making, and return the result.

pub mod apps;
pub mod backup;
pub mod devices;
pub mod health;
pub mod home_tracking;
pub mod install;
pub mod launcher;
pub mod loader;
pub mod optimize;
pub mod reboot;
pub mod recovery;
pub mod scan;
pub mod screenshot;
pub mod sideload;
pub mod snapshot;
pub mod state;
pub mod tuning;

pub use state::AppState;
