//! ADB driver — talks to devices via the `adb` subprocess.
//!
//! Per architectural commitment #3: every ADB invocation goes through this
//! module. No naked `adb ...` calls elsewhere. The driver normalizes output,
//! adds tracing, and surfaces errors via a typed `AdbError`.

pub mod driver;
pub mod install;
pub mod parse;
pub mod scan;

use tokio::process::Command;

/// Suppress the console window Windows flashes when a GUI process spawns a
/// console program (adb, route, …). Without this, every adb call from the app
/// pops a `cmd`-style window for a split second — a "waterfall" of them during
/// any multi-command action. `CREATE_NO_WINDOW` keeps the subprocess headless.
/// No-op on macOS/Linux, where spawning a subprocess never shows a window.
pub(crate) fn hide_console_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

pub use driver::{AdbDriver, AdbError, AdbOutput, AdbResult, SubprocessAdb};
pub use install::{adb_path_in_install_root, install_platform_tools, InstallError};
pub use parse::{
    parse_active_audio_device, parse_device_list, parse_disabled_packages_output,
    parse_display_mode, parse_hardware_properties_temp, parse_installed_packages_output,
    parse_ls_output, parse_meminfo_summary, parse_storage_info, parse_thermal_max_celsius,
    parse_total_pss_by_process, parse_usage_stats, AppUsage, DisplayMode, FileEntry, RamInfo,
    StorageInfo,
};
pub use scan::{local_subnet_prefix, scan_subnet, ScanHit};
