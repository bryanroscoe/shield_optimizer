//! Shared Shield Optimizer / ATV Optimizer core.
//!
//! This crate owns the pure safety engine, driver-generic ADB abstractions, and
//! shared Tauri command handlers used by the desktop and future Android app.

// clippy 1.99 added `double_must_use`, which fires on the futures that
// `#[async_trait]` generates for every trait method. The attribute is the
// macro's, not ours, so there is nothing to change at the call sites.
#![allow(clippy::double_must_use)]
pub mod adb;
pub mod commands;
pub mod engine;
pub mod license;
