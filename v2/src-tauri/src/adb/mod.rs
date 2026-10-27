//! Desktop ADB plumbing.
//!
//! Shared traits and parsers live in `shield_optimizer_core::adb`; this module
//! owns only the subprocess driver plus desktop-only platform-tools install and
//! network scan helpers.

#[cfg(feature = "direct-adb")]
pub mod direct;
#[cfg(feature = "direct-adb")]
mod discovery;
pub mod driver;
pub mod install;
pub mod scan;
pub mod selection;

use std::sync::{Arc, LazyLock};

use shield_optimizer_core::adb::Leases;
use tokio::process::Command;

/// Every adb process the app spawns, through any driver instance, counted
/// until it is reaped. The quit stop waits for this to drain.
pub fn child_leases() -> &'static Arc<Leases> {
    static CHILDREN: LazyLock<Arc<Leases>> = LazyLock::new(Leases::new);
    &CHILDREN
}

/// Write an executable test script from a separate process. A file this
/// process had open for writing can fail to exec with "text file busy" when
/// another test thread forks at the same moment.
#[cfg(all(test, unix))]
pub(crate) fn write_script(path: &std::path::Path, body: &str) {
    let written = std::process::Command::new("sh")
        .args([
            "-c",
            "printf '%s' \"$1\" > \"$2\" && chmod 755 \"$2\"",
            "sh",
        ])
        .arg(format!("#!/bin/sh\n{body}\n"))
        .arg(path)
        .status()
        .unwrap();
    assert!(written.success());
}

/// Suppress the console window Windows flashes when a GUI process spawns a
/// console program (adb, route, …). Without this, every adb call from the app
/// pops a `cmd`-style window for a split second — a "waterfall" of them during
/// any multi-command action. `CREATE_NO_WINDOW` keeps the subprocess headless.
/// No-op on macOS/Linux, where spawning a subprocess never shows a window.
pub(crate) fn hide_console_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        // tokio's Command has its own `creation_flags` on Windows, so the
        // std `CommandExt` trait does not need importing — and importing it
        // trips `-D unused-imports` in the Windows CI job.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Pin every adb subprocess to a stable working directory.
///
/// A child process inherits its parent's cwd, and `adb start-server` forks a
/// daemon that outlives this app entirely. When the app is launched from a
/// mounted DMG or with a removable volume as its cwd, that daemon goes on
/// holding a `/Volumes/...` path after the window is closed — which on macOS
/// keeps re-triggering the "would like to access files on a removable volume"
/// prompt with no app left to grant it (GitHub #89).
///
/// adb resolves every path we pass it absolutely, so it has no need of the
/// launch directory. Point the child at the user's home directory, falling
/// back to the filesystem root, so nothing we spawn can pin a volume the user
/// might eject.
pub(crate) fn pin_working_directory(cmd: &mut Command) {
    let stable = dirs::home_dir()
        .filter(|home| home.is_dir())
        .unwrap_or_else(|| std::path::PathBuf::from(std::path::MAIN_SEPARATOR_STR));
    cmd.current_dir(stable);
}

pub use driver::{cached_adb_binary, discover_adb_binary, forget_cached_adb_binary, SubprocessAdb};
pub use install::{adb_path_in_install_root, install_platform_tools, InstallError};
pub use scan::{local_subnet_prefix, scan_subnet, ScanHit, ADB_NETWORK_PORT};
pub use shield_optimizer_core::adb::{
    parse_active_audio_device, parse_device_list, parse_disabled_packages_output,
    parse_display_mode, parse_dumpsys_meminfo, parse_hardware_properties_temp,
    parse_installed_packages_output, parse_ls_output, parse_mdns_services, parse_meminfo_summary,
    parse_permission_granted, parse_storage_info, parse_thermal_max_celsius,
    parse_total_pss_by_process, parse_usage_stats, AdbDriver, AdbError, AdbOutput, AdbResult,
    AppUsage, DisplayMode, FileEntry, MdnsService, RamInfo, RemoteInputSession, StorageInfo,
    MDNS_SERVICE_CONNECT, MDNS_SERVICE_LEGACY, MDNS_SERVICE_PAIRING,
};
