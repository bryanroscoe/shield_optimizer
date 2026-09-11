//! Device-list and profile commands.

use serde::Serialize;
use tauri::State;

use crate::adb::{parse_device_list, AdbDriver};
use crate::engine::{
    detect_device_type,
    types::{Device, DeviceProperties, DeviceStatus},
    DeviceType,
};

use super::AppState;

/// `list_devices` — invoke `adb devices`, parse, look up properties for each
/// authorized device, classify type, return structured list.
#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> Result<Vec<Device>, String> {
    list_devices_impl(state.inner()).await
}

/// Reusable implementation — callable from inside other commands without
/// the `State<'_, T>` lifetime constraint getting in the way.
pub async fn list_devices_impl(state: &AppState) -> Result<Vec<Device>, String> {
    let adb = state.adb_snapshot().await;
    let raw = adb
        .raw(&["devices"])
        .await
        .map_err(|e| format!("adb devices: {e}"))?;
    let entries = parse_device_list(&raw.stdout);

    let mut out: Vec<Device> = Vec::with_capacity(entries.len());
    for (idx, e) in entries.iter().enumerate() {
        let id = (idx + 1) as u32;
        // For non-authorized devices we can't query properties — we still
        // surface them in the list with a placeholder name.
        if e.status != DeviceStatus::Device {
            out.push(Device {
                id,
                serial: e.serial.clone(),
                name: e.serial.clone(),
                model: String::new(),
                device_type: DeviceType::Unknown,
                status: e.status,
                connection: e.connection,
                properties: None,
            });
            continue;
        }

        let props = harvest_properties(&*adb, &e.serial).await?;
        let device_type = detect_device_type(&props);

        // Friendly name: custom device_name if set, else brand-based.
        let name = if !props
            .friendly_name
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            props.friendly_name.clone().unwrap_or_default()
        } else if !props.brand.is_empty() {
            format!("{} Device", props.brand)
        } else {
            "Android TV".to_string()
        };

        let model = friendly_model_for(device_type, &props);

        out.push(Device {
            id,
            serial: e.serial.clone(),
            name,
            model,
            device_type,
            status: e.status,
            connection: e.connection,
            properties: Some(props),
        });
    }

    Ok(out)
}

/// `device_profile` — return the same payload `list_devices` would for a single
/// device, freshly refetched. Used by the Profile view to force a refresh.
#[tauri::command]
pub async fn device_profile(state: State<'_, AppState>, serial: String) -> Result<Device, String> {
    device_profile_impl(state.inner(), &serial).await
}

pub async fn device_profile_impl(state: &AppState, serial: &str) -> Result<Device, String> {
    let devices = list_devices_impl(state).await?;
    devices
        .into_iter()
        .find(|d| d.serial == serial)
        .ok_or_else(|| format!("device {serial} not found"))
}

/// `connect_device` — `adb connect <ip>:<port>`. Returns ADB's stdout/stderr
/// so the UI can surface the actual response.
#[tauri::command]
pub async fn connect_device(
    state: State<'_, AppState>,
    address: String,
) -> Result<ConnectResult, String> {
    connect_device_impl(state.inner(), &address).await
}

async fn connect_device_impl(state: &AppState, address: &str) -> Result<ConnectResult, String> {
    let target = normalize_connect_address(address)?;
    let adb = state.adb_snapshot().await;
    let out = adb
        .raw(&["connect", &target])
        .await
        .map_err(|e| format!("adb connect: {e}"))?;
    Ok(connect_result_from(&out))
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ConnectOutcome {
    Connected,
    Unauthorized,
    Failed,
}

fn classify_connect_output(combined: &str) -> ConnectOutcome {
    let s = combined.to_lowercase();
    if s.contains("failed to authenticate") {
        ConnectOutcome::Unauthorized
    } else if s.contains("connected to") && !s.contains("failed") && !s.contains("cannot") {
        ConnectOutcome::Connected
    } else {
        ConnectOutcome::Failed
    }
}

/// `adb connect` exits 0 even on "failed to connect", so classify by output
/// text (same rule as scan_network). Unauthorized counts as ok — the device
/// is connected and waiting for the user to approve this computer on the TV.
fn connect_result_from(out: &crate::adb::AdbOutput) -> ConnectResult {
    let combined = format!("{}\n{}", out.stdout, out.stderr).trim().to_string();
    let outcome = classify_connect_output(&combined);
    ConnectResult {
        ok: outcome != ConnectOutcome::Failed,
        message: if outcome == ConnectOutcome::Unauthorized {
            format!("{combined} — approve this computer on the TV, then refresh.")
        } else {
            combined
        },
    }
}

/// `disconnect_device` — `adb disconnect <serial>`.
#[tauri::command]
pub async fn disconnect_device(
    state: State<'_, AppState>,
    serial: String,
) -> Result<ConnectResult, String> {
    // A live remote-input session holds an open socket + forward to this
    // device — tear it down before dropping the connection.
    state.drop_remote_session(&serial).await;
    let adb = state.adb_snapshot().await;
    let out = adb
        .raw(&["disconnect", &serial])
        .await
        .map_err(|e| format!("adb disconnect: {e}"))?;
    Ok(ConnectResult {
        ok: out.success(),
        message: if out.stdout.is_empty() {
            out.stderr
        } else {
            out.stdout
        },
    })
}

#[derive(Serialize)]
pub struct ConnectResult {
    pub ok: bool,
    pub message: String,
}

/// `pair_device` — Android 11+ pairing flow. Establishes trust over the
/// one-shot pairing port the TV displays alongside a 6-digit PIN. Connecting
/// is a separate step using the IP and port on the main Wireless debugging
/// screen; modern Android devices do not necessarily listen on port 5555.
///
/// `pair_address` is the IP:port shown on the TV's pairing screen.
/// `pin` is the 6-digit code, validated as digits only.
#[tauri::command]
pub async fn pair_device(
    state: State<'_, AppState>,
    pair_address: String,
    pin: String,
) -> Result<ConnectResult, String> {
    pair_device_impl(state.inner(), &pair_address, &pin).await
}

async fn pair_device_impl(
    state: &AppState,
    pair_address: &str,
    pin: &str,
) -> Result<ConnectResult, String> {
    if let Err(message) = validate_pairing_pin(pin) {
        return Ok(ConnectResult { ok: false, message });
    }
    let target = normalize_pairing_address(pair_address)?;
    let adb = state.adb_snapshot().await;
    let pair_out = adb
        .raw(&["pair", &target, pin])
        .await
        .map_err(|e| format!("adb pair: {e}"))?;
    let combined = pair_out.combined().trim().to_string();
    if !combined.to_lowercase().contains("successfully paired") {
        return Ok(ConnectResult {
            ok: false,
            message: combined,
        });
    }

    Ok(ConnectResult {
        ok: true,
        message: "Paired successfully. Pairing established trust; to connect, enter the separate IP:port shown on the TV's main Wireless debugging screen in Connect IP."
            .to_string(),
    })
}

/// Validate a wireless-debugging pairing PIN. The message is surfaced verbatim
/// to the user, so keep it stable — shared by desktop `pair_device` and the
/// mobile `WirelessAdb::pair` path.
pub fn validate_pairing_pin(pin: &str) -> Result<(), String> {
    if pin.len() != 6 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err("PIN must be exactly 6 digits.".to_string());
    }
    Ok(())
}

fn normalize_pairing_address(address: &str) -> Result<String, String> {
    if !address.trim().contains(':') {
        return Err("pairing address must include the port shown on the TV".to_string());
    }
    normalize_connect_address(address)
}

/// Validate and normalize an `IP[:port]` string. Rejects empty input, IPs
/// with the wrong shape, and any port that's not a positive 16-bit number.
/// Returns the canonical `IP:port` string ADB expects.
pub fn normalize_connect_address(address: &str) -> Result<String, String> {
    let address = address.trim();
    if address.is_empty() {
        return Err("address is empty".to_string());
    }

    let (host, port) = match address.split_once(':') {
        Some((h, p)) => (h, p),
        None => (address, "5555"),
    };

    let octets: Vec<&str> = host.split('.').collect();
    if octets.len() != 4 {
        return Err(format!("not a valid IPv4 address: {host}"));
    }
    for o in &octets {
        match o.parse::<u8>() {
            Ok(_) => {}
            Err(_) => return Err(format!("invalid IP octet: {o}")),
        }
    }

    match port.parse::<u16>() {
        Ok(0) => Err(format!("port must be 1-65535, got {port}")),
        Ok(_) => Ok(format!("{host}:{port}")),
        Err(_) => Err(format!("invalid port: {port}")),
    }
}

/// Batch-query device properties in a single shell call (matches v1's
/// optimization). The exact prop set is the union of what v1 used in
/// `Get-Devices` and `Show-DeviceProfile`.
async fn harvest_properties(adb: &dyn AdbDriver, serial: &str) -> Result<DeviceProperties, String> {
    // Use a sentinel string to delimit each prop output line — robust against
    // empty values that would otherwise collapse adjacent lines.
    let cmd = "settings get global device_name; getprop ro.product.brand; \
               getprop ro.product.model; getprop ro.product.device; \
               getprop ro.product.manufacturer; getprop ro.build.version.release; \
               getprop ro.build.version.sdk; getprop ro.build.id; \
               getprop ro.board.platform; getprop ro.build.characteristics; \
               getprop ro.serialno";

    let out = adb
        .shell(serial, cmd)
        .await
        .map_err(|e| format!("device profile: {e}"))?;
    if out.stdout.trim().is_empty() || out.exit_code.is_some_and(|code| code != 0) {
        return Err(format!(
            "device profile unavailable: {}",
            out.combined().trim()
        ));
    }

    let lines: Vec<&str> = out.stdout.lines().collect();
    let get = |i: usize| -> String {
        lines
            .get(i)
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };

    // device_name's response can be the literal "null" or an Exception line —
    // treat those as "no friendly name set".
    let raw_friendly = get(0);
    let friendly_name = if raw_friendly.is_empty()
        || raw_friendly == "null"
        || raw_friendly.contains("Exception")
    {
        None
    } else {
        Some(raw_friendly)
    };

    Ok(DeviceProperties {
        friendly_name,
        brand: get(1),
        model: get(2),
        device_codename: get(3),
        manufacturer: get(4),
        android_release: get(5),
        sdk_level: get(6),
        build_id: get(7),
        board_platform: get(8),
        characteristics: get(9),
        serial_number: get(10),
    })
}

const MAX_DEVICE_NAME_LEN: usize = 64;

/// Validate and shell-quote a device name for `settings put global
/// device_name`. Printable ASCII only — the value rides through the
/// device-side shell, and ADB mangles non-ASCII inconsistently across
/// builds; an honest rejection beats a garbled name on the TV.
fn quote_device_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Device name is empty.".to_string());
    }
    if name.len() > MAX_DEVICE_NAME_LEN {
        return Err(format!(
            "Device name too long ({} > {MAX_DEVICE_NAME_LEN} chars).",
            name.len()
        ));
    }
    if let Some(bad) = name.chars().find(|c| !c.is_ascii() || c.is_ascii_control()) {
        return Err(format!(
            "Unsupported character {bad:?} — device names are limited to plain ASCII here."
        ));
    }
    Ok(format!("'{}'", name.replace('\'', r"'\''")))
}

/// `rename_device` — `settings put global device_name '<name>'`, the same
/// write the TV's Settings → About → Device name performs. Updates what
/// Google Home / Cast / this app display. Verified by reading the value
/// back. Cast/network discovery can take a while (or a reboot) to
/// re-broadcast the new name to other devices.
#[tauri::command]
pub async fn rename_device(
    state: State<'_, AppState>,
    serial: String,
    name: String,
) -> Result<crate::commands::apps::ActionResult, String> {
    use crate::commands::apps::ActionResult;

    let quoted = match quote_device_name(&name) {
        Ok(q) => q,
        Err(message) => return Ok(ActionResult { ok: false, message }),
    };

    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(
            &serial,
            &format!("settings put global device_name {quoted}"),
        )
        .await
        .map_err(|e| format!("settings put device_name: {e}"))?;
    if out.shell_reported_failure() {
        return Ok(ActionResult {
            ok: false,
            message: out.combined().trim().to_string(),
        });
    }

    // Read back — `settings put` exits quietly even when the write didn't
    // take (e.g. a restricted build), so the get is the real confirmation.
    let now = adb
        .shell(&serial, "settings get global device_name")
        .await
        .map_err(|e| format!("settings get device_name: {e}"))?;
    let current = now.stdout.trim();
    if current == name.trim() {
        Ok(ActionResult {
            ok: true,
            message: format!(
                "Renamed to \"{current}\". Cast / Google Home may take a while (or a reboot) \
                 to show the new name."
            ),
        })
    } else {
        Ok(ActionResult {
            ok: false,
            message: format!("Device still reports device_name = {current:?} after the write."),
        })
    }
}

fn friendly_model_for(device_type: DeviceType, props: &DeviceProperties) -> String {
    match device_type {
        DeviceType::Shield => {
            crate::engine::detection::shield_friendly_model(&props.device_codename)
        }
        DeviceType::GoogleTv => {
            if !props.model.is_empty() {
                props.model.clone()
            } else {
                "Google TV Device".to_string()
            }
        }
        DeviceType::Unknown => {
            if !props.model.is_empty() {
                props.model.clone()
            } else {
                "Unknown Device".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::{state_with, MockAdb};

    #[tokio::test]
    async fn lost_socket_during_profiling_is_not_an_authorized_device() {
        let state = state_with(
            MockAdb::default()
                .on_raw(
                    "devices",
                    "List of devices attached\n192.168.1.2:5555\tdevice\n",
                )
                .on_shell_err("getprop", "Connection to the TV was lost"),
        );
        let error = list_devices_impl(&state)
            .await
            .expect_err("profile must fail");
        assert!(error.contains("Connection to the TV was lost"));
    }

    #[tokio::test]
    async fn list_devices_impl_parses_authorized_and_unauthorized() {
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                "List of devices attached\n\
                 192.168.42.71:5555\tdevice\n\
                 192.168.42.143:5555\tunauthorized\n",
            )
            // harvest_properties' batched getprop — give a brand so the name
            // resolves; other props default.
            .on_shell("settings get global device_name", "Living Room\nNVIDIA\n");
        let state = state_with(mock);
        let devices = list_devices_impl(&state).await.unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "192.168.42.71:5555");
        assert_eq!(
            devices[0].status,
            crate::engine::types::DeviceStatus::Device
        );
        // Unauthorized device is surfaced with no properties.
        assert_eq!(
            devices[1].status,
            crate::engine::types::DeviceStatus::Unauthorized
        );
        assert!(devices[1].properties.is_none());
    }

    #[tokio::test]
    async fn successful_pair_establishes_trust_without_connecting_to_5555() {
        let mock = MockAdb::default().on_raw(
            "pair 192.168.42.71:43219 123456",
            "Successfully paired to 192.168.42.71:43219\n",
        );
        let raw_log = mock.raw_log();
        let state = state_with(mock);

        let result = pair_device_impl(&state, "192.168.42.71:43219", "123456")
            .await
            .unwrap();

        assert!(result.ok);
        assert_eq!(
            result.message,
            "Paired successfully. Pairing established trust; to connect, enter the separate IP:port shown on the TV's main Wireless debugging screen in Connect IP."
        );
        assert_eq!(
            *raw_log.lock().unwrap(),
            vec!["pair 192.168.42.71:43219 123456"]
        );
    }

    #[tokio::test]
    async fn failed_pair_remains_a_failure_and_does_not_connect() {
        let mock = MockAdb::default().on_raw(
            "pair 192.168.42.71:43219 123456",
            "Failed: Wrong password\n",
        );
        let raw_log = mock.raw_log();
        let state = state_with(mock);

        let result = pair_device_impl(&state, "192.168.42.71:43219", "123456")
            .await
            .unwrap();

        assert!(!result.ok);
        assert_eq!(result.message, "Failed: Wrong password");
        assert_eq!(
            *raw_log.lock().unwrap(),
            vec!["pair 192.168.42.71:43219 123456"]
        );
    }

    #[tokio::test]
    async fn invalid_pairing_pin_fails_before_adb() {
        let mock = MockAdb::default();
        let raw_log = mock.raw_log();
        let state = state_with(mock);

        let result = pair_device_impl(&state, "192.168.42.71:43219", "12345a")
            .await
            .unwrap();

        assert!(!result.ok);
        assert_eq!(result.message, "PIN must be exactly 6 digits.");
        assert!(raw_log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn explicit_connection_port_is_used_instead_of_the_pairing_port() {
        let mock = MockAdb::default().on_raw(
            "connect 192.168.42.71:37123",
            "connected to 192.168.42.71:37123\n",
        );
        let raw_log = mock.raw_log();
        let state = state_with(mock);

        let result = connect_device_impl(&state, "192.168.42.71:37123")
            .await
            .unwrap();

        assert!(result.ok);
        assert_eq!(result.message, "connected to 192.168.42.71:37123");
        assert_eq!(
            *raw_log.lock().unwrap(),
            vec!["connect 192.168.42.71:37123"]
        );
    }

    #[test]
    fn normalize_accepts_bare_ip() {
        assert_eq!(
            normalize_connect_address("192.168.42.71").unwrap(),
            "192.168.42.71:5555"
        );
    }

    #[test]
    fn normalize_accepts_ip_and_port() {
        assert_eq!(
            normalize_connect_address("10.0.0.1:5556").unwrap(),
            "10.0.0.1:5556"
        );
    }

    #[test]
    fn pairing_address_requires_the_tv_supplied_port() {
        assert_eq!(
            normalize_pairing_address("192.168.42.71").unwrap_err(),
            "pairing address must include the port shown on the TV"
        );
        assert_eq!(
            normalize_pairing_address("192.168.42.71:43219").unwrap(),
            "192.168.42.71:43219"
        );
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(
            normalize_connect_address("  192.168.1.1  ").unwrap(),
            "192.168.1.1:5555"
        );
    }

    #[test]
    fn normalize_rejects_empty() {
        assert!(normalize_connect_address("").is_err());
        assert!(normalize_connect_address("   ").is_err());
    }

    #[test]
    fn normalize_rejects_non_ipv4() {
        assert!(normalize_connect_address("foo.bar.baz").is_err());
        assert!(normalize_connect_address("192.168.1").is_err());
        assert!(normalize_connect_address("192.168.1.1.1").is_err());
        assert!(normalize_connect_address("999.999.999.999").is_err());
    }

    #[test]
    fn normalize_rejects_bad_port() {
        assert!(normalize_connect_address("192.168.1.1:0").is_err());
        assert!(normalize_connect_address("192.168.1.1:abc").is_err());
        assert!(normalize_connect_address("192.168.1.1:99999").is_err());
    }

    #[test]
    fn device_name_quoting_and_validation() {
        assert_eq!(
            quote_device_name("Living Room Shield").unwrap(),
            "'Living Room Shield'"
        );
        assert_eq!(quote_device_name("Bryan's TV").unwrap(), r"'Bryan'\''s TV'");
        // Trims before quoting.
        assert_eq!(quote_device_name("  Den  ").unwrap(), "'Den'");
        assert!(quote_device_name("").is_err());
        assert!(quote_device_name("   ").is_err());
        assert!(quote_device_name("naïve name").is_err());
        assert!(quote_device_name(&"x".repeat(65)).is_err());
    }
}
