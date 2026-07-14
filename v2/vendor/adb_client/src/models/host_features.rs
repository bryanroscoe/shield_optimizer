use std::fmt::Display;

/// Available host features.
#[derive(Debug, Eq, PartialEq)]
pub enum HostFeatures {
    /// Shell version 2.
    ShellV2,
    /// Command.
    Cmd,
}

impl Display for HostFeatures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShellV2 => write!(f, "ShellV2"),
            Self::Cmd => write!(f, "Cmd"),
        }
    }
}

impl TryFrom<&[u8]> for HostFeatures {
    type Error = String;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            b"shell_v2" => Ok(Self::ShellV2),
            b"cmd" => Ok(Self::Cmd),
            _ => Err(format!("Unknown value {value:?}")),
        }
    }
}
