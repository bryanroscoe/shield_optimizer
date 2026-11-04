//! Repair for an adb server that macOS has cut off from the local network.
//!
//! macOS 15+ charges the adb server's network use to the app that started it
//! (Apple TN3179). The server outlives the app: quit leaves it running unless
//! the user opts in, and always when the app restarts for an update. Once a
//! newer build of the app launches, macOS blocks every connection that old
//! server makes. `adb connect` fails with "No route to host" and connected TVs
//! drop to offline, while the running app, and any server it starts, still
//! reach the same TV. adb run from Terminal is exempt, which is why a manual
//! `adb kill-server` then `adb connect` works around it.
//!
//! So when a connect or pair fails with "No route to host" for an address this
//! process can reach itself, the server is the broken part: restart it once
//! and retry. Anything less certain leaves the server alone.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use shield_optimizer_core::adb::{AdbOutput, AdbResult};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Whether this process can reach an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// A TCP handshake completed or was refused: packets got there and back.
    Reachable,
    /// Every attempt failed with "No route to host".
    Unreachable,
    /// Anything else: a timeout, a name that does not resolve, another error.
    Unknown,
}

/// How the driver tests reachability. Tests substitute a fixed answer.
#[derive(Debug, Clone, Copy)]
pub enum ReachProbe {
    Network,
    #[cfg(test)]
    Fixed(Reach),
}

impl ReachProbe {
    pub async fn probe(self, target: &str) -> Reach {
        match self {
            ReachProbe::Network => app_reaches(target).await,
            #[cfg(test)]
            ReachProbe::Fixed(reach) => reach,
        }
    }
}

/// Serializes repairs across concurrent commands and remembers the last one,
/// so a burst of failing connects restarts the server once, and a server that
/// still fails right after a restart is not restarted again.
#[derive(Debug, Default)]
pub struct RepairGate {
    last: Mutex<Option<Instant>>,
}

/// After a restart, failures are not blamed on the server again for this long.
pub const REPAIR_COOLDOWN: Duration = Duration::from_secs(60);

/// What to do about a command that failed with "No route to host".
#[derive(Debug, PartialEq, Eq)]
pub enum Repair {
    /// Another command restarted the server after this one started: retry.
    Retry,
    /// Probe, and restart the server if this process reaches the target.
    Restart,
    /// The server was restarted moments ago and still fails; leave it.
    Leave,
}

impl RepairGate {
    /// Hold the gate while deciding and restarting, so concurrent commands
    /// wait for one restart instead of each starting their own.
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, Option<Instant>> {
        self.last.lock().await
    }
}

pub fn decide(last_restart: Option<Instant>, command_started: Instant) -> Repair {
    match last_restart {
        Some(at) if at >= command_started => Repair::Retry,
        Some(at) if at.elapsed() < REPAIR_COOLDOWN => Repair::Leave,
        _ => Repair::Restart,
    }
}

/// The target of a command whose "No route to host" can mean the server, not
/// the TV, is cut off. Both are dialled by the server itself.
pub fn repairable_target<'a>(args: &[&'a str]) -> Option<&'a str> {
    match args {
        ["connect", target] | ["pair", target, ..] => Some(target),
        _ => None,
    }
}

pub fn reports_no_route(result: &AdbResult<AdbOutput>) -> bool {
    let text = match result {
        Ok(out) => out.combined(),
        Err(error) => error.to_string(),
    };
    text.to_lowercase().contains("no route to host")
}

/// A newly launched build's first connection is refused this way for about a
/// second while macOS evaluates it, so the probe retries before concluding.
const PROBE_ATTEMPTS: u32 = 3;
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
const PROBE_RETRY_DELAY: Duration = Duration::from_secs(1);

/// adb's port when a connect target names none.
const DEFAULT_ADB_PORT: u16 = 5555;

/// `EHOSTUNREACH`. `ErrorKind::HostUnreachable` needs Rust 1.83, and this
/// crate supports 1.81.
const HOST_UNREACHABLE: Option<i32> = if cfg!(target_os = "macos") {
    Some(65)
} else if cfg!(target_os = "linux") {
    Some(113)
} else if cfg!(windows) {
    Some(10065)
} else {
    None
};

/// One TCP connection from this process to `target`, closed straight away.
/// adbd sees a connection that sends nothing, so the TV shows no prompt.
pub async fn app_reaches(target: &str) -> Reach {
    let address = with_port(target);
    for attempt in 1..=PROBE_ATTEMPTS {
        match timeout(PROBE_TIMEOUT, TcpStream::connect(address.as_str())).await {
            Ok(Ok(_)) => return Reach::Reachable,
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                return Reach::Reachable
            }
            Ok(Err(e)) if HOST_UNREACHABLE.is_some() && e.raw_os_error() == HOST_UNREACHABLE => {
                if attempt == PROBE_ATTEMPTS {
                    return Reach::Unreachable;
                }
                tokio::time::sleep(PROBE_RETRY_DELAY).await;
            }
            _ => return Reach::Unknown,
        }
    }
    Reach::Unknown
}

fn with_port(target: &str) -> String {
    if target.parse::<SocketAddr>().is_ok() {
        return target.to_string();
    }
    if let Ok(ip) = target.parse::<IpAddr>() {
        return SocketAddr::new(ip, DEFAULT_ADB_PORT).to_string();
    }
    if target.contains(':') {
        target.to_string()
    } else {
        format!("{target}:{DEFAULT_ADB_PORT}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shield_optimizer_core::adb::AdbError;

    fn output(stdout: &str) -> AdbResult<AdbOutput> {
        Ok(AdbOutput {
            stdout: stdout.into(),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }

    #[test]
    fn only_connect_and_pair_are_repairable() {
        assert_eq!(
            repairable_target(&["connect", "192.0.2.1:5555"]),
            Some("192.0.2.1:5555")
        );
        assert_eq!(
            repairable_target(&["pair", "192.0.2.1:37000", "123456"]),
            Some("192.0.2.1:37000")
        );
        assert_eq!(repairable_target(&["devices"]), None);
        assert_eq!(
            repairable_target(&["-s", "192.0.2.1:5555", "shell", "id"]),
            None
        );
        assert_eq!(repairable_target(&["connect"]), None);
    }

    #[test]
    fn no_route_is_recognized_in_output_and_errors() {
        assert!(reports_no_route(&output(
            "failed to connect to '192.0.2.1:5555': No route to host"
        )));
        assert!(reports_no_route(&Err(AdbError::NonZeroExit {
            code: Some(1),
            stderr: "failed to connect to '192.0.2.1:5555': No route to host".into(),
        })));
        assert!(!reports_no_route(&output("connected to 192.0.2.1:5555")));
        assert!(!reports_no_route(&output(
            "failed to connect to '192.0.2.1:5555': Connection refused"
        )));
        assert!(!reports_no_route(&Err(AdbError::Timeout { seconds: 30 })));
    }

    #[test]
    fn a_restart_after_the_command_started_means_retry() {
        let started = Instant::now();
        let restarted = started + Duration::from_millis(1);
        assert_eq!(decide(Some(restarted), started), Repair::Retry);
    }

    #[test]
    fn a_recent_restart_before_the_command_is_not_repeated() {
        let restarted = Instant::now();
        let started = restarted + Duration::from_millis(1);
        assert_eq!(decide(Some(restarted), started), Repair::Leave);
    }

    #[test]
    fn no_restart_yet_or_an_old_one_allows_a_restart() {
        let started = Instant::now();
        assert_eq!(decide(None, started), Repair::Restart);
        if let Some(old) = Instant::now().checked_sub(REPAIR_COOLDOWN + Duration::from_secs(1)) {
            assert_eq!(decide(Some(old), started), Repair::Restart);
        }
    }

    #[test]
    fn targets_without_a_port_use_adbs_default() {
        assert_eq!(with_port("192.0.2.1"), "192.0.2.1:5555");
        assert_eq!(with_port("192.0.2.1:40123"), "192.0.2.1:40123");
        assert_eq!(with_port("fe80::1"), "[fe80::1]:5555");
        assert_eq!(with_port("[fe80::1]:40123"), "[fe80::1]:40123");
        assert_eq!(with_port("tv.local"), "tv.local:5555");
    }

    #[tokio::test]
    async fn a_listening_port_is_reachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let target = listener.local_addr().unwrap().to_string();
        assert_eq!(app_reaches(&target).await, Reach::Reachable);
    }

    // Windows retries a refused loopback connection for about two seconds,
    // past the probe's timeout. The probe only runs on macOS.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_refused_connection_still_proves_the_host_is_reachable() {
        let closed = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        assert_eq!(app_reaches(&closed.to_string()).await, Reach::Reachable);
    }

    #[tokio::test]
    async fn a_name_that_does_not_resolve_claims_nothing() {
        assert_eq!(app_reaches("name.invalid:5555").await, Reach::Unknown);
    }
}
