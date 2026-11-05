/// How a shell command's end was reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellExit {
    /// Shell protocol v2 delivered the command's exit status.
    Code(u8),
    /// The device does not speak shell protocol v2, and the legacy stream has
    /// no exit status. Nothing is known about success; judge the output.
    Unreported,
}

impl ShellExit {
    /// The exit status, if the device reported one.
    #[must_use]
    pub const fn code(self) -> Option<u8> {
        match self {
            Self::Code(code) => Some(code),
            Self::Unreported => None,
        }
    }
}
