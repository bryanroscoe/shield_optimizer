use std::fmt::Display;

#[derive(Debug, Copy, Clone)]
/// Type of reboot needed.
pub enum RebootType {
    /// "Classic" device reboot
    System,
    /// Reboots to bootloader
    Bootloader,
    /// Reboots to recovery
    Recovery,
    /// Reboots into recovery and automatically starts sideload mode
    Sideload,
    /// Same as `Sideload` but reboots after sideloading
    SideloadAutoReboot,
    /// Reboots to fastboot
    Fastboot,
}

impl Display for RebootType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, ""),
            Self::Bootloader => write!(f, "bootloader"),
            Self::Recovery => write!(f, "recovery"),
            Self::Sideload => write!(f, "sideload"),
            Self::SideloadAutoReboot => write!(f, "sideload-auto-reboot"),
            Self::Fastboot => write!(f, "fastboot"),
        }
    }
}
