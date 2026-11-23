//! Desktop-only Tauri command handlers.
//!
//! Shared command handlers live in `atv_optimizer_core::commands`; this
//! module keeps commands that depend on desktop filesystem paths, platform-tools
//! installation, the desktop updater, host-network scanning, or macOS Local
//! Network privacy, plus the Wake-on-LAN send.

pub mod adb_engine;
pub mod app_report;
pub mod backup;
pub mod diagnostics;
pub mod files;
pub mod install;
pub mod local_network;
pub mod quit;
pub mod scan;
pub mod sideload;
pub mod update;
pub mod wake;

#[cfg(test)]
pub use atv_optimizer_core::commands::test_support;
pub use atv_optimizer_core::commands::AppState;
