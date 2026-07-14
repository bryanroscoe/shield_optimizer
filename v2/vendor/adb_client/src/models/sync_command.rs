use std::fmt::Display;

pub enum SyncCommand {
    /// List files in a folder
    List,
    /// Receive a file from the device
    Recv,
    /// Send a file to the device
    Send,
    // Stat a file
    Stat,
}

impl Display for SyncCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::List => write!(f, "LIST"),
            Self::Recv => write!(f, "RECV"),
            Self::Send => write!(f, "SEND"),
            Self::Stat => write!(f, "STAT"),
        }
    }
}
