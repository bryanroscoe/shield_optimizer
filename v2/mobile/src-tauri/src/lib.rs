//! ATV Optimizer mobile Tauri entry point.
//!
//! This crate wires the shared core to an Android wireless-ADB transport. Host
//! builds compile the same code with a stub transport so CI can validate the
//! Rust command surface before Android hardware is available.

mod adb_plugin;
mod wireless_adb;
mod wireless_commands;

use std::path::PathBuf;
use std::sync::Arc;

use shield_optimizer_core::commands::{
    apps, devices, health, input, launcher, loader, reboot, recovery, screenshot, AppState,
};
use shield_optimizer_core::engine;
use shield_optimizer_core::license::Entitlement;
use tauri::Manager;
use wireless_adb::WirelessAdb;
use wireless_commands::MobileState;

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ATVOptimizer")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(adb_plugin::init())
        .setup(|app| {
            let app_lists = match loader::load_embedded_app_lists() {
                Ok(lists) => {
                    tracing::info!(total = lists.total(), "app lists loaded");
                    lists
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to load embedded app lists");
                    engine::AppListBundle::default()
                }
            };

            let wireless = Arc::new(WirelessAdb::new(app.handle().clone()));
            let state = AppState::new(wireless.clone(), app_lists, default_data_dir())
                .with_known_names(loader::load_known_names())
                .with_entitlement(Entitlement::Free);
            app.manage(state);
            app.manage(MobileState { wireless });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            wireless_commands::wireless_discover,
            wireless_commands::wireless_pair,
            wireless_commands::wireless_connect,
            wireless_commands::wireless_disconnect,
            devices::list_devices,
            devices::device_profile,
            devices::rename_device,
            health::health_report,
            health::app_list_for_device,
            health::report_all,
            launcher::list_launchers,
            launcher::current_launcher,
            launcher::channel_provider_disabled,
            apps::force_stop,
            screenshot::take_screenshot,
            apps::package_states,
            apps::list_other_packages,
            apps::app_memory_map,
            apps::app_usage_map,
            apps::safety_info,
            apps::trim_caches,
            input::send_text,
            input::send_key,
            input::open_settings,
            recovery::panic_recovery,
            reboot::reboot_device,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ATV Optimizer mobile application");
}
