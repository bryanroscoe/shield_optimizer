//! Shared ADB abstractions and output parsers.

pub mod driver;
pub mod parse;
pub mod remote_input;

pub use driver::{AdbByteStream, AdbDriver, AdbError, AdbOutput, AdbResult};
pub use parse::{
    parse_active_audio_device, parse_device_list, parse_disabled_packages_output,
    parse_display_mode, parse_dumpsys_meminfo, parse_hardware_properties_temp,
    parse_installed_packages_output, parse_ls_output, parse_meminfo_summary,
    parse_permission_granted, parse_storage_info, parse_thermal_max_celsius,
    parse_total_pss_by_process, parse_usage_stats, AppUsage, DisplayMode, FileEntry, RamInfo,
    StorageInfo,
};
pub use remote_input::RemoteInputSession;
