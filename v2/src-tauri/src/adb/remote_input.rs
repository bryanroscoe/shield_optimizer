//! Persistent scrcpy control-socket session for low-latency remote input.
//!
//! Today each remote key press shells out `adb shell "input keyevent N"`, which
//! cold-starts a JVM on the TV (~690 ms per press). This module replaces the
//! transport with scrcpy's control-only server: push the jar once, run it
//! resident via `app_process`, and stream fixed binary control messages over a
//! forwarded TCP socket — dropping per-press cost to network RTT.
//!
//! Layering note: every adb invocation (push / forward / the `app_process`
//! spawn) goes through the `AdbDriver` trait, per the one-wrapper rule. The raw
//! `TcpStream` to the forwarded local port is a DOCUMENTED exception to that
//! rule — the same kind of exception `scan.rs` makes for its route/ip probes.
//! There is no way to carry scrcpy's binary control protocol over the driver's
//! line-oriented `shell`; the socket is the protocol.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Child;
use tracing::{debug, warn};

use crate::adb::AdbDriver;

/// The scrcpy protocol version this jar implements. MUST equal the version
/// baked into `resources/scrcpy-server-v3.1` or the server aborts on launch.
const SCRCPY_VERSION: &str = "3.1";

/// Where the server jar is pushed on the device.
const DEVICE_JAR_PATH: &str = "/data/local/tmp/shieldopt-scrcpy-server.jar";

/// Bundled jar location, relative to both the Tauri resource root (for
/// `BaseDirectory::Resource`) and the crate root (dev fallback). Resolved at
/// runtime by `commands::state::resolve_scrcpy_server_jar`.
pub const SERVER_JAR_RESOURCE_PATH: &str = "resources/scrcpy-server-v3.1";

/// scrcpy control message type bytes.
const TYPE_INJECT_KEYCODE: u8 = 0;
const TYPE_INJECT_TEXT: u8 = 1;

/// Android `KeyEvent` actions.
const ACTION_DOWN: u8 = 0;
const ACTION_UP: u8 = 1;

/// scrcpy caps a single INJECT_TEXT at 300 chars; longer strings are clamped.
const MAX_TEXT_CHARS: usize = 300;

/// How many times to retry the TCP connect before giving up. With
/// `tunnel_forward=true` the server LISTENS on the localabstract socket and we
/// connect after `adb forward`, so there is a brief startup race.
const CONNECT_ATTEMPTS: usize = 10;
const CONNECT_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(150);

/// Encode an INJECT_KEYCODE control message (always 14 bytes, big-endian):
/// `[type=0][action][u32 keycode][u32 repeat][u32 metaState]`.
pub fn encode_inject_keycode(action: u8, keycode: u32, repeat: u32, meta_state: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(14);
    buf.push(TYPE_INJECT_KEYCODE);
    buf.push(action);
    buf.extend_from_slice(&keycode.to_be_bytes());
    buf.extend_from_slice(&repeat.to_be_bytes());
    buf.extend_from_slice(&meta_state.to_be_bytes());
    buf
}

/// Encode an INJECT_TEXT control message: `[type=1][u32 byte-len][UTF-8 bytes]`.
/// Text is clamped to 300 chars (scrcpy's cap) before encoding; the length
/// prefix is the UTF-8 byte length of the clamped string.
pub fn encode_inject_text(text: &str) -> Vec<u8> {
    let clamped: String = text.chars().take(MAX_TEXT_CHARS).collect();
    let bytes = clamped.as_bytes();
    let mut buf = Vec::with_capacity(5 + bytes.len());
    buf.push(TYPE_INJECT_TEXT);
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
    buf
}

/// Format a scid as scrcpy expects: 8 lowercase hex digits, masked to 31 bits
/// (the high bit must be clear — the server parses scid as a positive int and
/// treats a negative value as "no scid").
fn format_scid(value: u32) -> String {
    format!("{:08x}", value & 0x7fff_ffff)
}

/// Process-wide counter mixed into the time seed so two sessions started within
/// the same millisecond still get distinct scids.
static SCID_COUNTER: AtomicU32 = AtomicU32::new(0);

fn next_scid() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let bump = SCID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format_scid(nanos ^ bump.wrapping_mul(2654435761))
}

/// `adb -s <serial> forward tcp:<port> localabstract:scrcpy_<scid>` argument
/// vector. The `-s` is load-bearing: without it, `adb forward` errors with
/// "more than one device/emulator" whenever a second device is connected.
fn forward_args(serial: &str, port: u16, scid: &str) -> Vec<String> {
    vec![
        "-s".to_string(),
        serial.to_string(),
        "forward".to_string(),
        format!("tcp:{port}"),
        format!("localabstract:scrcpy_{scid}"),
    ]
}

/// `adb -s <serial> forward --remove tcp:<port>` argument vector.
fn forward_remove_args(serial: &str, port: u16) -> Vec<String> {
    vec![
        "-s".to_string(),
        serial.to_string(),
        "forward".to_string(),
        "--remove".to_string(),
        format!("tcp:{port}"),
    ]
}

/// `adb -s <serial> push <jar> <device-path>` argument vector.
fn push_args(serial: &str, local_jar: &str) -> Vec<String> {
    vec![
        "-s".to_string(),
        serial.to_string(),
        "push".to_string(),
        local_jar.to_string(),
        DEVICE_JAR_PATH.to_string(),
    ]
}

/// The full `adb` argument vector that launches the resident server:
/// `-s <serial> shell CLASSPATH=<jar> app_process / com.genymobile.scrcpy.Server <ver> scid=… …`.
/// Pure + deterministic given `serial`/`scid` so it can be asserted byte-for-byte.
fn server_spawn_args(serial: &str, scid: &str) -> Vec<String> {
    vec![
        "-s".to_string(),
        serial.to_string(),
        "shell".to_string(),
        format!("CLASSPATH={DEVICE_JAR_PATH}"),
        "app_process".to_string(),
        "/".to_string(),
        "com.genymobile.scrcpy.Server".to_string(),
        SCRCPY_VERSION.to_string(),
        format!("scid={scid}"),
        "log_level=info".to_string(),
        "video=false".to_string(),
        "audio=false".to_string(),
        "control=true".to_string(),
        "tunnel_forward=true".to_string(),
        "send_device_meta=false".to_string(),
        "send_dummy_byte=true".to_string(),
    ]
}

/// Reserve a free local TCP port by binding to `:0` and reading back the
/// kernel-assigned port. There's a small TOCTOU window between drop and `adb
/// forward`, accepted as standard practice.
fn pick_free_local_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("scrcpy: could not reserve a local port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("scrcpy: could not read local port: {e}"))?
        .port();
    Ok(port)
}

/// Connect to the forwarded port AND complete the handshake, retrying the
/// whole sequence. The retry must cover the dummy-byte read, not just the
/// connect: with `adb forward`, the local adb daemon accepts the TCP
/// connection immediately even while the device-side socket isn't listening
/// yet, then closes it (EOF). Only a successful 0x00 read proves the server
/// is actually on the other end — same dance scrcpy's own client does.
async fn connect_with_retry(port: u16) -> Result<TcpStream, String> {
    let mut last_err = String::new();
    for attempt in 0..CONNECT_ATTEMPTS {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(mut stream) => {
                let mut dummy = [0u8; 1];
                match tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    stream.read_exact(&mut dummy),
                )
                .await
                {
                    Ok(Ok(_)) if dummy[0] == 0x00 => return Ok(stream),
                    Ok(Ok(_)) => {
                        return Err(format!(
                            "scrcpy: unexpected handshake byte {:#04x} (expected 0x00)",
                            dummy[0]
                        ))
                    }
                    Ok(Err(e)) => last_err = format!("handshake read: {e}"),
                    Err(_) => last_err = "handshake read timed out".to_string(),
                }
            }
            Err(e) => last_err = e.to_string(),
        }
        if attempt + 1 < CONNECT_ATTEMPTS {
            tokio::time::sleep(CONNECT_RETRY_DELAY).await;
        }
    }
    Err(format!(
        "scrcpy: control channel never came up on 127.0.0.1:{port} after \
         {CONNECT_ATTEMPTS} attempts: {last_err}"
    ))
}

/// A live scrcpy control session bound to one device serial. Holds the open
/// control socket and the resident server child for the lifetime of the Remote
/// tab; the server exits the instant the socket closes, so the session must
/// keep both alive and tear down explicitly.
pub struct RemoteInputSession {
    stream: TcpStream,
    /// Kept alive for the session; `kill_on_drop(true)` reaps the server.
    _child: Child,
    scid: String,
    port: u16,
    serial: String,
    /// Driver retained for async teardown (`adb forward --remove`).
    adb: Arc<dyn AdbDriver>,
}

impl RemoteInputSession {
    /// Push the server jar, forward a fresh local port to its control socket,
    /// spawn the resident server, connect, and consume the handshake dummy byte.
    pub async fn start(
        adb: Arc<dyn AdbDriver>,
        jar_path: &Path,
        serial: &str,
    ) -> Result<Self, String> {
        let local_jar = jar_path
            .to_str()
            .ok_or_else(|| "scrcpy: server jar path is not valid UTF-8".to_string())?;

        let push = push_args(serial, local_jar);
        adb.raw_transfer(&as_str_args(&push))
            .await
            .map_err(|e| format!("scrcpy: push server jar: {e}"))?;

        let scid = next_scid();
        let port = pick_free_local_port()?;

        let forward = forward_args(serial, port, &scid);
        adb.raw(&as_str_args(&forward))
            .await
            .map_err(|e| format!("scrcpy: adb forward tcp:{port}: {e}"))?;

        let spawn = server_spawn_args(serial, &scid);
        let child = match adb.spawn(&as_str_args(&spawn)).await {
            Ok(child) => child,
            Err(e) => {
                // Don't leak the forward we just created.
                let _ = adb
                    .raw(&as_str_args(&forward_remove_args(serial, port)))
                    .await;
                return Err(format!("scrcpy: spawn control server: {e}"));
            }
        };

        let session = Self {
            // Connect failure tears down explicitly (forward-remove + child
            // kill) rather than leaking the resources start() just created.
            stream: match connect_with_retry(port).await {
                Ok(stream) => stream,
                Err(e) => {
                    let mut child = child;
                    let _ = child.start_kill();
                    let _ = adb
                        .raw(&as_str_args(&forward_remove_args(serial, port)))
                        .await;
                    return Err(e);
                }
            },
            _child: child,
            scid,
            port,
            serial: serial.to_string(),
            adb,
        };

        // The handshake dummy byte was already consumed inside
        // connect_with_retry — the stream is ready for control messages.
        debug!(
            serial = %session.serial,
            scid = %session.scid,
            port = session.port,
            "scrcpy control session established"
        );
        Ok(session)
    }

    /// Inject a single key-down or key-up event.
    pub async fn send_key(&mut self, keycode: u32, down: bool) -> Result<(), String> {
        let action = if down { ACTION_DOWN } else { ACTION_UP };
        let msg = encode_inject_keycode(action, keycode, 0, 0);
        self.write_all(&msg).await
    }

    /// Inject a full key press (down then up).
    pub async fn send_key_press(&mut self, keycode: u32) -> Result<(), String> {
        self.send_key(keycode, true).await?;
        self.send_key(keycode, false).await
    }

    /// Inject UTF-8 text (clamped to 300 chars).
    pub async fn send_text(&mut self, text: &str) -> Result<(), String> {
        let msg = encode_inject_text(text);
        self.write_all(&msg).await
    }

    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.stream
            .write_all(bytes)
            .await
            .map_err(|e| format!("scrcpy: control socket write failed: {e}"))?;
        self.stream
            .flush()
            .await
            .map_err(|e| format!("scrcpy: control socket flush failed: {e}"))
    }

    /// Graceful teardown. Order matters (verified on device): close the control
    /// socket FIRST — the server exits on its own the instant the socket drops —
    /// then a belt-and-suspenders `pkill` for any lingering server process, then
    /// reap the local child and remove the adb forward. `Drop` is the
    /// best-effort backstop; prefer this.
    pub async fn close(mut self) {
        let _ = self.stream.shutdown().await;
        // Match our jar's name, not "com.genymobile.scrcpy" — the broad pattern
        // would also kill a desktop scrcpy session the user has open against
        // this same device.
        let _ = self
            .adb
            .shell(&self.serial, "pkill -f shieldopt-scrcpy-server")
            .await;
        let _ = self._child.start_kill();
        let _ = self
            .adb
            .raw(&as_str_args(&forward_remove_args(&self.serial, self.port)))
            .await;
    }
}

impl Drop for RemoteInputSession {
    fn drop(&mut self) {
        // `_child` has kill_on_drop(true), so the server dies and the control
        // socket closes here. Best-effort removal of the adb forward entry if a
        // tokio runtime is live (e.g. app exit dropping the session registry).
        let adb = self.adb.clone();
        let serial = self.serial.clone();
        let port = self.port;
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let args = forward_remove_args(&serial, port);
                let _ = adb.raw(&as_str_args(&args)).await;
            });
        } else {
            warn!(
                port,
                "scrcpy: no runtime in Drop; adb forward entry may leak"
            );
        }
    }
}

/// Borrow a `Vec<String>` as the `&[&str]` the driver wants.
fn as_str_args(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn inject_keycode_down_select_is_exact_14_bytes() {
        // SELECT/center = 23, action down, repeat 0, meta 0.
        let got = encode_inject_keycode(ACTION_DOWN, 23, 0, 0);
        assert_eq!(
            got,
            vec![
                0x00, 0x00, 0x00, 0x00, 0x00, 0x17, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
        assert_eq!(got.len(), 14);
    }

    #[test]
    fn inject_keycode_up_dpad_with_repeat_and_meta() {
        // D-pad up = 19 (0x13), action up = 1, repeat = 2, meta = 0x1001.
        let got = encode_inject_keycode(ACTION_UP, 19, 2, 0x0000_1001);
        assert_eq!(
            got,
            vec![
                0x00, // type
                0x01, // action up
                0x00, 0x00, 0x00, 0x13, // keycode 19
                0x00, 0x00, 0x00, 0x02, // repeat 2
                0x00, 0x00, 0x10, 0x01, // meta
            ]
        );
    }

    #[test]
    fn inject_text_encodes_type_len_then_utf8() {
        let got = encode_inject_text("hi");
        assert_eq!(got, vec![0x01, 0x00, 0x00, 0x00, 0x02, b'h', b'i']);
    }

    #[test]
    fn inject_text_length_is_utf8_byte_count_not_char_count() {
        // "é" is 2 UTF-8 bytes — the length prefix must be 2, not 1.
        let got = encode_inject_text("é");
        assert_eq!(got[0], TYPE_INJECT_TEXT);
        assert_eq!(&got[1..5], &[0x00, 0x00, 0x00, 0x02]);
        assert_eq!(&got[5..], "é".as_bytes());
    }

    #[test]
    fn inject_text_clamps_to_300_chars() {
        let long = "a".repeat(500);
        let got = encode_inject_text(&long);
        // 1 type byte + 4 length bytes + 300 payload bytes.
        assert_eq!(got.len(), 5 + 300);
        assert_eq!(&got[1..5], &300u32.to_be_bytes());
    }

    #[test]
    fn format_scid_is_8_hex_digits_with_high_bit_cleared() {
        assert_eq!(format_scid(0), "00000000");
        assert_eq!(format_scid(0x0000_00ff), "000000ff");
        // High bit is masked off.
        assert_eq!(format_scid(0xffff_ffff), "7fffffff");
        assert_eq!(format_scid(0x8000_0001), "00000001");
    }

    #[test]
    fn server_spawn_args_match_phase0_launch_line() {
        assert_eq!(
            server_spawn_args("ABC123", "0a1b2c3d"),
            vec![
                "-s",
                "ABC123",
                "shell",
                "CLASSPATH=/data/local/tmp/shieldopt-scrcpy-server.jar",
                "app_process",
                "/",
                "com.genymobile.scrcpy.Server",
                "3.1",
                "scid=0a1b2c3d",
                "log_level=info",
                "video=false",
                "audio=false",
                "control=true",
                "tunnel_forward=true",
                "send_device_meta=false",
                "send_dummy_byte=true",
            ]
        );
    }

    #[test]
    fn forward_and_push_args_are_well_formed() {
        assert_eq!(
            forward_args("ABC123", 41000, "0a1b2c3d"),
            vec![
                "-s",
                "ABC123",
                "forward",
                "tcp:41000",
                "localabstract:scrcpy_0a1b2c3d"
            ]
        );
        assert_eq!(
            forward_remove_args("ABC123", 41000),
            vec!["-s", "ABC123", "forward", "--remove", "tcp:41000"]
        );
        assert_eq!(
            push_args("ABC123", "/tmp/scrcpy-server-v3.1"),
            vec![
                "-s",
                "ABC123",
                "push",
                "/tmp/scrcpy-server-v3.1",
                "/data/local/tmp/shieldopt-scrcpy-server.jar",
            ]
        );
    }

    #[test]
    fn pick_free_local_port_returns_a_usable_port() {
        let port = pick_free_local_port().expect("should reserve a port");
        assert!(port > 0);
    }

    // Honest error-path coverage: with a driver that can't spawn a child
    // (MockAdb uses the trait's default `spawn`, which returns unsupported),
    // start() must surface the spawn failure rather than hang or fake success.
    // The live socket path is left for the lead's on-device check.
    #[tokio::test]
    async fn start_fails_when_driver_cannot_spawn() {
        use crate::commands::test_support::MockAdb;
        let adb: Arc<dyn AdbDriver> = Arc::new(MockAdb::default());
        let jar = std::path::PathBuf::from("/tmp/scrcpy-server-v3.1");
        let result = RemoteInputSession::start(adb, &jar, "SERIAL123").await;
        match result {
            Ok(_) => panic!("start must fail when the driver cannot spawn the server"),
            Err(err) => assert!(
                err.contains("spawn control server"),
                "unexpected error: {err}"
            ),
        }
    }

    // Live end-to-end test against a real device — ignored in CI, run by hand:
    //   SHIELD_TEST_SERIAL=192.168.x.x:5555 cargo test remote_input_live -- --ignored --nocapture
    // Starts a real session, injects SLEEP (223) and verifies via `dumpsys
    // power` that the device went to sleep, then WAKEUP (224) and verifies it
    // woke, then tears down and checks no server process is left behind.
    #[tokio::test]
    #[ignore]
    async fn remote_input_live_roundtrip() {
        let Ok(serial) = std::env::var("SHIELD_TEST_SERIAL") else {
            panic!("set SHIELD_TEST_SERIAL to run this live test");
        };
        let adb: Arc<dyn AdbDriver> = Arc::new(
            crate::adb::SubprocessAdb::discover().expect("adb binary required for live test"),
        );
        let jar = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(super::SERVER_JAR_RESOURCE_PATH);

        async fn wakefulness(adb: &dyn AdbDriver, serial: &str) -> String {
            adb.shell(serial, "dumpsys power | grep -m1 mWakefulness")
                .await
                .map(|o| o.stdout.trim().rsplit('=').next().unwrap_or("").to_string())
                .unwrap_or_default()
        }

        let mut session = RemoteInputSession::start(adb.clone(), &jar, &serial)
            .await
            .expect("session start");

        let t0 = std::time::Instant::now();
        session.send_key_press(223).await.expect("inject SLEEP");
        let mut state = String::new();
        for _ in 0..40 {
            state = wakefulness(adb.as_ref(), &serial).await;
            if state == "Asleep" {
                break;
            }
        }
        println!(
            "SLEEP -> {state} in {:?} (incl. dumpsys polling)",
            t0.elapsed()
        );
        assert_eq!(state, "Asleep", "device should have gone to sleep");

        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        session.send_key_press(224).await.expect("inject WAKEUP");
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert_eq!(
            wakefulness(adb.as_ref(), &serial).await,
            "Awake",
            "device should have woken"
        );

        session.close().await;
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let leftovers = adb
            .shell(&serial, "ps -A | grep shieldopt-scrcpy || true")
            .await
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default();
        assert!(
            leftovers.is_empty(),
            "server process left behind after close(): {leftovers}"
        );
    }
}
