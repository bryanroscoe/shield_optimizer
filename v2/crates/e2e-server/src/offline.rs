//! Reviewed command boundary for the fixture-only E2E server.

pub const FIXTURE_SUBNET: &str = "192.0.2";

// New command handlers need an offline-I/O review before entering this list.
pub fn allowed(command: &str) -> bool {
    matches!(
        command,
        "telemetry_settings"
            | "telemetry_action"
            | "telemetry_error"
            | "list_devices"
            | "device_profile"
            | "connect_device"
            | "disconnect_device"
            | "pair_device"
            | "forget_device"
            | "reprompt_device"
            // The simulator has no native engine, so both answer Unsupported.
            | "request_device_approval"
            | "cancel_device_approval"
            // Reads `adb devices` from the simulator and nothing else.
            | "transport_state"
            | "probe_paired_connect"
            | "connect_paired"
            | "local_address_for"
            // Both answered by `intercepted`: no probe or System Settings launch.
            | "local_network_access"
            | "open_local_network_settings"
            | "rename_device"
            | "health_report"
            | "media_report"
            | "resource_sample"
            | "app_list_for_device"
            | "report_all"
            | "adb_status"
            | "install_adb"
            | "restart_adb"
            | "get_stop_adb_on_quit"
            | "set_stop_adb_on_quit"
            // Reviewed offline-safe: the status reads managed state only, and the
            // E2E server refuses a switch without swapping out the simulator.
            | "adb_engine"
            | "set_adb_engine"
            | "scan_network"
            | "list_launchers"
            | "current_launcher"
            | "channel_provider_disabled"
            | "set_default_launcher"
            | "disable_launcher"
            | "set_home_any"
            | "disable_stock_launcher"
            | "disable_setup_helper"
            | "disable_package"
            | "enable_package"
            | "force_stop"
            | "take_screenshot"
            | "uninstall_package"
            | "reinstall_existing"
            | "open_play_store"
            | "package_states"
            | "list_other_packages"
            | "list_installed_packages"
            | "app_memory_map"
            | "app_storage_map"
            | "app_apk_size"
            | "app_usage_map"
            | "safety_info"
            | "process_safety_info"
            | "trim_caches"
            | "app_permission_state"
            | "set_app_permission"
            | "set_app_op"
            | "get_app_op"
            | "remote_open"
            | "remote_close"
            | "send_text"
            | "send_key"
            | "open_settings"
            | "install_apk"
            | "list_apks_in_folder"
            | "inspect_apk"
            | "backup_apk"
            | "clone_app"
            | "list_dir"
            | "pull_file"
            | "push_file"
            | "delete_path"
            | "find_files"
            | "copy_file_to_device"
            | "list_snapshots"
            | "save_snapshot"
            | "preview_apply"
            | "apply_snapshot"
            | "delete_snapshot"
            | "snapshot_dir_path"
            | "panic_recovery"
            | "reboot_device"
            | "run_shell"
            | "read_device_logs"
            | "get_tweaks"
            | "write_setting"
            | "set_display_scaling"
            | "get_display_scaling"
            | "get_private_dns"
            | "set_private_dns"
            | "prepare_optimize"
            | "apply_performance_settings"
            | "check_for_update"
            // Answered by `intercepted`.
            | "update_channel"
            | "save_app_report"
            | "collect_diagnostics"
            | "set_debug_logging"
            | "get_debug_logging"
            | "log_dir_path"
            | "open_log_dir"
            | "log_ui_event"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_hub_cannot_send_even_if_enabled() {
        let hub = atv_optimizer_core::telemetry::Telemetry::disabled();
        assert!(!hub.settings().available);
        assert!(!hub.is_active());
        hub.set_enabled(true).unwrap();
        assert!(!hub.settings().available);
        assert!(!hub.is_active());
    }

    #[test]
    fn unknown_and_plugin_commands_cannot_cross_the_offline_boundary() {
        assert!(allowed("telemetry_settings"));
        assert!(allowed("telemetry_action"));
        assert!(allowed("telemetry_error"));
        assert!(!allowed("set_telemetry_enabled"));
        assert!(!allowed("dismiss_telemetry_notice"));
        assert!(allowed("remote_open"));
        assert!(allowed("scan_network"));
        // Answered by the server's intercept table, never by the host.
        assert!(allowed("local_network_access"));
        assert!(allowed("open_local_network_settings"));
        assert!(allowed("adb_engine"));
        assert!(allowed("set_adb_engine"));
        assert!(!allowed("new_unreviewed_command"));
        assert!(!allowed("plugin:opener|open_url"));
    }
}
