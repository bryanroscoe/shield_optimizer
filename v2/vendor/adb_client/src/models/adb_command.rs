use std::fmt::Display;

use crate::models::{ADBHostCommand, ADBLocalCommand};

/// Represent all ADB commands available (either through a server (host) or a device (local)).
pub enum ADBCommand {
    /// Commands executed on an ADB server.
    Host(ADBHostCommand),
    /// Commands executed on an ADB device.
    Local(ADBLocalCommand),
}

impl Display for ADBCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(host_command) => write!(f, "{host_command}"),
            Self::Local(adb_local_command) => write!(f, "{adb_local_command}"),
        }
    }
}

#[test]
fn test_pair_command() {
    let host = "192.168.0.197:34783";
    let code = "091102";
    let code_u32 = code.parse::<u32>().expect("cannot parse u32");
    let pair = ADBCommand::Host(ADBHostCommand::Pair(
        host.parse().expect("cannot parse host"),
        code.into(),
    ));

    assert_eq!(pair.to_string(), format!("host:pair:{code}:{host}"));
    assert_ne!(pair.to_string(), format!("host:pair:{code_u32}:{host}"));
}
