use std::fmt::Display;

pub enum ADBEmulatorCommand {
    Authenticate(String),
    AvdDiscoveryPath,
    AvdGrpcPort,
    Sms(String, String),
    Rotate,
    Raw(String),
}

impl Display for ADBEmulatorCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Need to call `writeln!` because emulator commands are '\n' terminated
        match self {
            Self::Authenticate(token) => writeln!(f, "auth {token}"),
            Self::AvdDiscoveryPath => writeln!(f, "avd discoverypath"),
            Self::AvdGrpcPort => writeln!(f, "avd grpc"),
            Self::Sms(phone_number, content) => {
                writeln!(f, "sms send {phone_number} {content}")
            }
            Self::Rotate => writeln!(f, "rotate"),
            Self::Raw(command) => writeln!(f, "{command}"),
        }
    }
}
