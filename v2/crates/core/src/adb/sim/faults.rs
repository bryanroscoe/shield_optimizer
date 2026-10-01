//! Fault injection for the simulated device.
//!
//! A rule matches by substring against one *simple* device command (so a
//! fault inside a batched read fires on that section alone) or, with
//! `scope: adb`, against the joined host-side adb arguments (`connect …`,
//! `pair …`). Rules are consumed in order; `times` bounds how often one fires
//! and `after` lets the first N matches through untouched.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FaultScope {
    /// One simple command inside an `adb shell` string.
    #[default]
    Shell,
    /// The whole `adb …` argument list, joined with spaces.
    Adb,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FaultEffect {
    /// Answer with this output and status; state is untouched.
    Fail {
        #[serde(default)]
        stdout: String,
        #[serde(default)]
        stderr: String,
        #[serde(default = "one")]
        exit_code: i32,
    },
    /// Report success but change nothing — the accept-but-ignore setter.
    Ignore {
        #[serde(default)]
        stdout: String,
    },
    /// Replace the output; state is untouched.
    Output {
        stdout: String,
        #[serde(default)]
        exit_code: i32,
    },
    /// The whole adb invocation times out (after `delay_ms` of real time).
    Timeout {
        #[serde(default)]
        delay_ms: u64,
    },
    /// Answer normally, `ms` later.
    Delay { ms: u64 },
}

fn one() -> i32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaultRule {
    /// Hardware serial (`ro.serialno`) the rule is limited to.
    #[serde(default)]
    pub serial: Option<String>,
    #[serde(default)]
    pub scope: FaultScope,
    /// Substring of the command text.
    pub matches: String,
    pub effect: FaultEffect,
    /// Fire at most this many times (`None` = every time).
    #[serde(default)]
    pub times: Option<u32>,
    /// Let this many matches through before firing.
    #[serde(default)]
    pub after: u32,
    /// How often the rule has fired, for the control endpoint to report.
    #[serde(default)]
    pub fired: u32,
}

/// The effect of the first rule that fires for `text`, if any.
pub(crate) fn take(
    rules: &mut [FaultRule],
    scope: FaultScope,
    serial: &str,
    text: &str,
) -> Option<FaultEffect> {
    for rule in rules.iter_mut() {
        if rule.scope != scope || !text.contains(&rule.matches) {
            continue;
        }
        if rule.serial.as_deref().is_some_and(|s| s != serial) {
            continue;
        }
        if rule.after > 0 {
            rule.after -= 1;
            continue;
        }
        if rule.times.is_some_and(|t| rule.fired >= t) {
            continue;
        }
        rule.fired += 1;
        return Some(rule.effect.clone());
    }
    None
}
