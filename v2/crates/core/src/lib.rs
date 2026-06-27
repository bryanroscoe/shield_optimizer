//! Shared Shield Optimizer / ATV Optimizer core.
//!
//! This crate owns the pure safety engine, driver-generic ADB abstractions, and
//! shared Tauri command handlers used by the desktop and future Android app.

pub mod adb;
pub mod commands;
pub mod engine;
pub mod license;
