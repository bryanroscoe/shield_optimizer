use std::fmt::Display;

use crate::RebootType;

/// ADB commands that relates to an actual device.
pub enum ADBLocalCommand {
    ShellCommand(String, Vec<String>),
    Shell,
    Exec(String),
    Sync,
    Reboot(RebootType),
    Forward(String, String),
    ForwardRemove(String),
    ForwardRemoveAll,
    Reverse(String, String),
    ReverseRemove(String),
    ReverseRemoveAll,
    Reconnect,
    Remount,
    DisableVerity,
    EnableVerity,
    Uninstall(String, Option<String>),
    Install(u64, Option<String>),
    TcpIp(u16),
    Usb,
    Root,

    #[cfg(feature = "framebuffer")]
    FrameBuffer,
}

impl Display for ADBLocalCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sync => write!(f, "sync:"),
            Self::ShellCommand(command, shell_args) => {
                if shell_args.is_empty() {
                    // Shell v1: simple format for older ADB versions
                    write!(f, "shell:{command}")
                } else {
                    // Shell v2: with arguments
                    let args_s = shell_args.join(",");
                    write!(f, "shell,{args_s},raw:{command}")
                }
            }
            Self::Shell => match std::env::var("TERM") {
                Ok(term) => write!(f, "shell,TERM={term},raw:"),
                Err(_) => write!(f, "shell,raw:"),
            },
            Self::Exec(command) => write!(f, "exec:{command}"),
            Self::Reboot(reboot_type) => {
                write!(f, "reboot:{reboot_type}")
            }
            Self::Uninstall(package, user) => {
                write!(f, "exec:cmd package 'uninstall'")?;
                if let Some(user) = user {
                    write!(f, " --user {user}")?;
                }
                write!(f, " {package}")
            }
            Self::Install(size, user) => {
                write!(f, "exec:cmd package 'install'")?;
                if let Some(user) = user {
                    write!(f, " --user {user}")?;
                }
                write!(f, " -S {size}")
            }
            Self::Forward(remote, local) => {
                write!(f, "host:forward:{local};{remote}")
            }
            Self::ForwardRemove(local) => write!(f, "host:killforward:{local}"),
            Self::ForwardRemoveAll => write!(f, "host:killforward-all"),
            Self::Reverse(remote, local) => {
                write!(f, "reverse:forward:{remote};{local}")
            }
            Self::ReverseRemove(remote) => write!(f, "reverse:killforward:{remote}"),
            Self::ReverseRemoveAll => write!(f, "reverse:killforward-all"),
            Self::Reconnect => write!(f, "reconnect"),
            Self::Remount => write!(f, "remount:"),
            Self::DisableVerity => write!(f, "disable-verity:"),
            Self::EnableVerity => write!(f, "enable-verity:"),
            Self::TcpIp(port) => {
                write!(f, "tcpip:{port}")
            }
            Self::Usb => write!(f, "usb:"),
            Self::Root => write!(f, "root:"),

            #[cfg(feature = "framebuffer")]
            Self::FrameBuffer => write!(f, "framebuffer:"),
        }
    }
}

#[test]
fn test_forward_remove_command() {
    let command = ADBLocalCommand::ForwardRemove("tcp:7100".to_string());

    assert_eq!(command.to_string(), "host:killforward:tcp:7100");
}

#[test]
fn test_reverse_remove_command() {
    let command = ADBLocalCommand::ReverseRemove("tcp:7100".to_string());

    assert_eq!(command.to_string(), "reverse:killforward:tcp:7100");
}
