use std::{str::FromStr, sync::LazyLock};

use regex::Regex;

use crate::{Result, RustADBError};

static REMOUNT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^Using\s+(?P<path>\S+)\s+for\s+(?P<mode>\S+)$").expect("Invalid remount regex")
});

#[derive(Debug)]
/// Information about remount operation
pub struct RemountInfo {
    /// Path that was remounted
    pub path: String,
    /// Mode that was used for remounting
    pub mode: String,
}

impl FromStr for RemountInfo {
    type Err = RustADBError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let caps = REMOUNT_REGEX
            .captures(s)
            .ok_or_else(|| RustADBError::RemountError(s.to_string()))?;

        let (Some(path), Some(mode)) = (caps.name("path"), caps.name("mode")) else {
            return Err(RustADBError::RemountError(s.to_string()));
        };

        Ok(Self {
            path: path.as_str().to_string(),
            mode: mode.as_str().to_string(),
        })
    }
}

impl RemountInfo {
    pub(crate) fn from_str_response(s: &str) -> Result<Vec<Self>> {
        if !s.ends_with("remount succeeded") {
            return Err(RustADBError::RemountError(s.to_string()));
        }

        let mut infos = Vec::new();
        for line in s.lines() {
            if line.starts_with("Using") {
                infos.push(Self::from_str(line)?);
            }
        }

        Ok(infos)
    }
}
