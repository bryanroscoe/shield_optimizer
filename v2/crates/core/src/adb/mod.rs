//! Shared ADB abstractions and output parsers.

pub mod batch;
pub mod driver;
pub mod parse;
pub mod remote_input;

pub use batch::{batch_command, split_batch, BATCH_SEPARATOR};
pub use batch::{checked_batch_command, parse_checked_batch};
pub use driver::{
    AdbByteStream, AdbDriver, AdbError, AdbOutput, AdbResult, BoundedShellOutput, ShellTermination,
};
pub use parse::{
    instance_serial, is_network_endpoint, kb_to_mb, package_for_process, parse_active_audio_device,
    parse_device_list, parse_disabled_packages_output, parse_display_mode, parse_display_modes,
    parse_dumpsys_meminfo, parse_hardware_properties_temp, parse_installed_packages_output,
    parse_ls_output, parse_mdns_services, parse_meminfo_summary, parse_net_dev,
    parse_permission_granted, parse_proc_stat, parse_pss_by_process, parse_storage_info,
    parse_thermal_max_celsius, parse_total_pss_by_process, parse_usage_stats, AppUsage, CpuSample,
    DisplayMode, FileEntry, MdnsService, NetSample, ProcessMemory, RamInfo, StorageInfo,
    MDNS_SERVICE_CONNECT, MDNS_SERVICE_LEGACY, MDNS_SERVICE_PAIRING,
};
pub use remote_input::RemoteInputSession;
