use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogPriority {
    Verbose,
    Debug,
    #[default]
    Info,
    Warning,
    Error,
    Fatal,
}

impl LogPriority {
    fn flag(self) -> char {
        match self {
            Self::Verbose => 'V',
            Self::Debug => 'D',
            Self::Info => 'I',
            Self::Warning => 'W',
            Self::Error => 'E',
            Self::Fatal => 'F',
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogcatOptions {
    pub priority: LogPriority,
    pub lines: u16,
    pub tag: Option<String>,
    pub package: Option<String>,
}

impl LogcatOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=1000).contains(&self.lines) {
            return Err("Choose between 1 and 1000 recent log entries.".into());
        }
        if let Some(tag) = &self.tag {
            if tag.is_empty()
                || tag.starts_with('-')
                || tag.len() > 64
                || !tag
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
            {
                return Err(
                    "A log tag must be 1–64 letters, digits, dots, underscores or hyphens, and cannot start with a hyphen.".into(),
                );
            }
        }
        if let Some(package) = &self.package {
            if package.len() > 255 || !super::launcher::is_valid_package_name(package) {
                return Err("Choose a valid Android package name.".into());
            }
        }
        Ok(())
    }

    pub fn command(&self, pid: Option<u32>) -> Result<String, String> {
        self.validate()?;
        if self.package.is_some() != pid.is_some() || pid == Some(0) {
            return Err("An app filter requires exactly one running main-process ID.".into());
        }
        let mut command = format!("logcat -d -v threadtime -t {}", self.lines);
        if let Some(pid) = pid {
            command.push_str(&format!(" --pid={pid}"));
        }
        match &self.tag {
            Some(tag) => command.push_str(&format!(" '{tag}:{}' '*:S'", self.priority.flag())),
            None => command.push_str(&format!(" '*:{}'", self.priority.flag())),
        }
        Ok(command)
    }
}

pub fn parse_main_pid(text: &str) -> Result<u32, String> {
    let words: Vec<_> = text.split_whitespace().collect();
    if words.len() != 1 {
        return Err(
            "The app has no single running main process. Start it on the TV, then retry.".into(),
        );
    }
    let value = words[0];
    if !value.chars().all(|c| c.is_ascii_digit()) {
        return Err("The TV did not return a valid process ID.".into());
    }
    value
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| "The TV did not return a valid process ID.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> LogcatOptions {
        LogcatOptions {
            priority: LogPriority::Info,
            lines: 200,
            tag: None,
            package: None,
        }
    }

    #[test]
    fn snapshots_are_finite_and_never_clear_or_change_logging() {
        assert_eq!(
            options().command(None).unwrap(),
            "logcat -d -v threadtime -t 200 '*:I'"
        );
    }

    #[test]
    fn a_tag_is_an_allowlist_not_an_additional_global_filter() {
        let mut o = options();
        o.tag = Some("ActivityManager".into());
        o.priority = LogPriority::Warning;
        assert_eq!(
            o.command(None).unwrap(),
            "logcat -d -v threadtime -t 200 'ActivityManager:W' '*:S'"
        );
    }

    #[test]
    fn unresolved_app_filter_never_falls_back_to_all_apps() {
        let mut o = options();
        o.package = Some("com.example.app".into());
        assert!(o.command(None).is_err());
        assert!(o.command(Some(0)).is_err());
        assert!(o.command(Some(123)).unwrap().contains(" --pid=123 "));
        assert!(options().command(Some(123)).is_err());
    }

    #[test]
    fn filters_cannot_inject_shell_words_or_logcat_flags() {
        for bad in [
            "", "*:V", "x y", "x';id", "$(id)", "x\ny", "-c", "--clear", "-G",
        ] {
            let mut o = options();
            o.tag = Some(bad.into());
            assert!(o.validate().is_err(), "{bad:?}");
        }
        for bad in ["", "com.example;id", "com.example app", "--pid=1", "$(id)"] {
            let mut o = options();
            o.package = Some(bad.into());
            assert!(o.validate().is_err(), "{bad:?}");
        }
        for lines in [0, 1001, u16::MAX] {
            let mut o = options();
            o.lines = lines;
            assert!(o.validate().is_err());
        }
    }

    #[test]
    fn process_resolution_is_strict() {
        assert_eq!(parse_main_pid("123\n").unwrap(), 123);
        for bad in [
            "",
            "0",
            "1 2",
            "+123",
            "1;id",
            "pidof: not found",
            "4294967296",
        ] {
            assert!(parse_main_pid(bad).is_err(), "{bad:?}");
        }
    }
}
