use std::{fmt::Display, io::BufRead, str::FromStr, sync::LazyLock};

use chrono::{DateTime, Utc};
use regex::{Captures, Regex};

use crate::RustADBError;

/// Represents a mapping between an unix id and name in the stat response from an ADB device.
#[derive(Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct ADBStatMapping {
    /// The unix id of the user or group.
    pub id: u32,
    /// The name of the user or group.
    pub name: String,
}

/// Represents the extended stat response from an ADB device.
#[derive(Debug)]
pub struct ADBStatExtendedResponse {
    /// The path of the file.
    pub path: String,
    /// The size of the file in bytes.
    pub size: u64,
    /// The number of blocks allocated for the file.
    pub blocks: u64,
    /// The number of IO blocks allocated for the file.
    pub io_blocks: u64,
    /// The inode number of the file.
    pub inode: u64,
    /// The number of hard links to the file.
    pub links: u64,
    /// The permissions of the file.
    pub perms: u64,
    /// The user who owns the file.
    pub user: ADBStatMapping,
    /// The group that owns the file.
    pub group: ADBStatMapping,
    /// The last access time of the file.
    pub atime: u32,
    /// The last modification time of the file.
    pub mtime: u32,
    /// The last status change time of the file.
    pub ctime: u32,
}

impl Display for ADBStatExtendedResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "File: {}", self.path)?;
        writeln!(
            f,
            "Size: {}\tBlocks: {}\t IO blocks: {}",
            self.size, self.blocks, self.io_blocks
        )?;
        writeln!(f, "Inode: {}\tLinks: {}", self.inode, self.links)?;
        writeln!(
            f,
            "Access: ({})\tUid: ({}/{})\tGid: ({}/{})",
            self.perms, self.user.id, self.user.name, self.group.id, self.group.name
        )?;
        writeln!(
            f,
            "Access: {}",
            DateTime::<Utc>::from_timestamp(i64::from(self.atime), 0)
                .unwrap_or(DateTime::UNIX_EPOCH),
        )?;
        writeln!(
            f,
            "Modify: {}",
            DateTime::<Utc>::from_timestamp(i64::from(self.mtime), 0)
                .unwrap_or(DateTime::UNIX_EPOCH),
        )?;
        writeln!(
            f,
            "Change: {}",
            DateTime::<Utc>::from_timestamp(i64::from(self.ctime), 0)
                .unwrap_or(DateTime::UNIX_EPOCH)
        )?;

        Ok(())
    }
}

static SECOND_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "^\\s+Size: (?P<size>\\d+)\\s+Blocks: (?P<blocks>\\d+)\\s+IO Blocks: (?P<io_blocks>\\d+).*$",
    )
    .expect("wrong syntax for second line regex")
});

static THIRD_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("^.*Inode: (?P<inode>\\d+)\\s+Links: (?P<links>\\d+).*$")
        .expect("wrong syntax for third regex")
});

static FOURTH_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("^Access: \\((?P<perms>\\d+).*\\)\\s+Uid: \\(\\s(?P<uid>\\d+)/\\s+(?P<uid_name>.*)\\)\\s+Gid: \\(\\s(?P<gid>\\d+)/\\s+(?P<gid_name>.*)\\)$")
        .expect("wrong syntax for fourth regex")
});

impl ADBStatExtendedResponse {
    /// Tries to parse an [`AdbStatExtendedResponse`] from the given bytes.
    /// Returns `Ok(None)` if the file does not exist on the device.
    pub(crate) fn try_from(value: &[u8]) -> crate::Result<Option<Self>> {
        fn extract_from_regex_group<T: FromStr>(
            groups: &Captures,
            name: &str,
        ) -> Result<T, RustADBError> {
            groups
                .name(name)
                .ok_or_else(|| RustADBError::StatResponseError(format!("no group named {name}")))?
                .as_str()
                .parse::<T>()
                .map_err(|_| RustADBError::StatResponseError("cannot parse value".into()))
        }

        fn parse_date(date: &str) -> crate::Result<u32> {
            let date: DateTime<Utc> = date.trim().parse()?;
            Ok(u32::try_from(date.timestamp())?)
        }

        let mut iter_lines = value.lines();

        let first_line = iter_lines
            .next()
            .ok_or_else(|| RustADBError::StatResponseError("no first line".into()))??;
        // leading spaces are preserved
        let path = match first_line.strip_prefix("  File: ") {
            Some(path) => path.trim(),
            None if first_line.ends_with("No such file or directory") => {
                // file does not exist on device
                return Ok(None);
            }
            None => return Err(RustADBError::StatResponseError("invalid path line".into())),
        };

        let second_line = iter_lines
            .next()
            .ok_or_else(|| RustADBError::StatResponseError("no second line".into()))??;

        let second_line_groups = SECOND_LINE_REGEX.captures(&second_line).ok_or_else(|| {
            RustADBError::StatResponseError("cannot get capture groups for second line".into())
        })?;
        let (size, blocks, io_blocks) = (
            extract_from_regex_group(&second_line_groups, "size")?,
            extract_from_regex_group(&second_line_groups, "blocks")?,
            extract_from_regex_group(&second_line_groups, "io_blocks")?,
        );

        let third_line = iter_lines.next().ok_or_else(|| {
            RustADBError::UnknownResponseType("stat response: no third line".into())
        })??;
        let third_line_groups = THIRD_LINE_REGEX.captures(&third_line).ok_or_else(|| {
            RustADBError::StatResponseError("cannot get capture groups for third line".into())
        })?;
        let (inode, links) = (
            extract_from_regex_group(&third_line_groups, "inode")?,
            extract_from_regex_group(&third_line_groups, "links")?,
        );

        let fourth_line = iter_lines.next().ok_or_else(|| {
            RustADBError::UnknownResponseType("stat response: no fourth line".into())
        })??;
        let fourth_line_groups = FOURTH_LINE_REGEX.captures(&fourth_line).ok_or_else(|| {
            RustADBError::StatResponseError("cannot get capture groups for fourth line".into())
        })?;
        let (perms, uid, uid_name, gid, gid_name) = (
            extract_from_regex_group(&fourth_line_groups, "perms")?,
            extract_from_regex_group(&fourth_line_groups, "uid")?,
            extract_from_regex_group(&fourth_line_groups, "uid_name")?,
            extract_from_regex_group(&fourth_line_groups, "gid")?,
            extract_from_regex_group(&fourth_line_groups, "gid_name")?,
        );

        let fifth_line = iter_lines.next().ok_or_else(|| {
            RustADBError::UnknownResponseType("stat response: no fifth line".into())
        })??;
        let atime = parse_date(fifth_line.strip_prefix("Access: ").ok_or_else(|| {
            RustADBError::UnknownResponseType("stat response: invalid atime line".into())
        })?)?;

        let sixth_line = iter_lines.next().ok_or_else(|| {
            RustADBError::UnknownResponseType("stat response: no sixth line".into())
        })??;
        let mtime = parse_date(sixth_line.strip_prefix("Modify: ").ok_or_else(|| {
            RustADBError::UnknownResponseType("stat response: invalid mtime line".into())
        })?)?;

        let seventh_line = iter_lines.next().ok_or_else(|| {
            RustADBError::UnknownResponseType("stat response: no seventh line".into())
        })??;
        let ctime = parse_date(seventh_line.strip_prefix("Change: ").ok_or_else(|| {
            RustADBError::UnknownResponseType("stat response: invalid ctime line".into())
        })?)?;

        Ok(Some(Self {
            path: path.to_string(),
            size,
            blocks,
            io_blocks,
            inode,
            links,
            perms,
            user: ADBStatMapping {
                id: uid,
                name: uid_name,
            },
            group: ADBStatMapping {
                id: gid,
                name: gid_name,
            },
            atime,
            mtime,
            ctime,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::{ADBStatExtendedResponse, models::adb_stat_extended_response::ADBStatMapping};

    #[test]
    fn parse_stat_extended_response() {
        let response = r"  File: /data/local/tmp/bigfile
  Size: 1048576	 Blocks: 2048	 IO Blocks: 512	 regular file
Device: fe3ch/65084d	 Inode: 45880	 Links: 1	 Device type: 0,0
Access: (0777/-rwxrwxrwx)	Uid: ( 2000/   shell)	Gid: ( 2000/   shell)
Access: 1970-01-01 01:00:00.000000000 +0100
Modify: 1970-01-01 01:00:00.000000000 +0100
Change: 2024-11-28 16:27:23.276724566 +0100
";
        let resp = ADBStatExtendedResponse::try_from(response.as_bytes())
            .expect("cannot parse stat extended response")
            .expect("no such file or directory");

        assert_eq!(resp.path, "/data/local/tmp/bigfile");
        assert_eq!(resp.size, 1_048_576);
        assert_eq!(resp.blocks, 2048);
        assert_eq!(resp.io_blocks, 512);
        assert_eq!(resp.links, 1);
        assert_eq!(
            resp.user,
            ADBStatMapping {
                id: 2000,
                name: "shell".to_string()
            }
        );
        assert_eq!(
            resp.group,
            ADBStatMapping {
                id: 2000,
                name: "shell".to_string()
            }
        );
        assert_eq!(resp.atime, 0);
        assert_eq!(resp.mtime, 0);
        assert_eq!(resp.ctime, 1_732_807_643);
    }
}
