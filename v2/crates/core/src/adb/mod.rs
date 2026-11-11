//! Shared ADB abstractions and output parsers.

pub mod batch;
pub mod driver;
pub mod lifetimes;
#[cfg(not(target_os = "android"))]
pub(crate) mod native_remote;
pub mod parse;
pub mod remote_input;
pub mod remote_lifecycle;
#[cfg(any(test, feature = "test-support"))]
pub mod sim;
pub mod storage;
pub mod upload;

pub use batch::{batch_command, split_batch, BATCH_SEPARATOR};
pub use batch::{checked_batch_command, parse_checked_batch, BATCH_STATUS};
pub use driver::{
    AdbByteStream, AdbDeviceBinding, AdbDriver, AdbError, AdbOutput, AdbOwnedService, AdbResult,
    AdbServiceCancellation, ApprovalRequestOutcome, BoundedShellOutput, NativeAdvertisement,
    NativeDiscoveryReport, NativeServiceKind, ScanConnectOutcome, ScanConnectionFailure,
    ScanConnectionProgress, ServerStop, ShellTermination,
};
pub use lifetimes::{Lease, Leases, TrackedChild};
pub use parse::{
    instance_serial, is_network_endpoint, is_reportable_instance_name, kb_to_mb,
    native_mdns_services, package_for_process, pair_output_guid, parse_active_audio_device,
    parse_device_list, parse_disabled_packages_output, parse_display_mode, parse_display_modes,
    parse_dumpsys_meminfo, parse_hardware_properties_reading, parse_hardware_properties_temp,
    parse_installed_packages_output, parse_ls_output, parse_mdns_services, parse_meminfo_summary,
    parse_net_dev, parse_permission_granted, parse_proc_stat, parse_pss_by_process,
    parse_storage_info, parse_thermal_max_celsius, parse_thermal_reading, parse_thermal_status,
    parse_total_pss_by_process, parse_usage_stats, render_mdns_services, AppUsage, CpuSample,
    DisplayMode, FileEntry, MdnsService, NetSample, ProcessMemory, RamInfo, StorageInfo,
    TemperatureReading, MDNS_SERVICE_CONNECT, MDNS_SERVICE_LEGACY, MDNS_SERVICE_PAIRING,
};
pub use remote_input::RemoteInputSession;
pub use storage::{
    parse_diskstats_package_sizes, parse_pm_path_apks, parse_stat_sizes_total, AppStorage,
};
