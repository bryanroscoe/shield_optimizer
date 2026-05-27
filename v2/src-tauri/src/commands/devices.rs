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

        let props = harvest_properties(&*adb, &e.serial).await;
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
    let target = normalize_connect_address(&address)?;
    let adb = state.adb_snapshot().await;
    let out = adb
        .raw(&["connect", &target])
        .await
        .map_err(|e| format!("adb connect: {e}"))?;
    Ok(ConnectResult {
        ok: out.success(),
        message: if out.stdout.is_empty() {
            out.stderr
        } else {
            out.stdout
        },
    })
}

/// `disconnect_device` — `adb disconnect <serial>`.
#[tauri::command]
pub async fn disconnect_device(
    state: State<'_, AppState>,
    serial: String,
) -> Result<ConnectResult, String> {
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

/// Validate and normalize an `IP[:port]` string. Rejects empty input, IPs
/// with the wrong shape, and any port that's not a positive 16-bit number.
/// Returns the canonical `IP:port` string ADB expects.
pub(crate) fn normalize_connect_address(address: &str) -> Result<String, String> {
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
async fn harvest_properties(adb: &dyn AdbDriver, serial: &str) -> DeviceProperties {
    // Use a sentinel string to delimit each prop output line — robust against
    // empty values that would otherwise collapse adjacent lines.
    let cmd = "settings get global device_name; getprop ro.product.brand; \
               getprop ro.product.model; getprop ro.product.device; \
               getprop ro.product.manufacturer; getprop ro.build.version.release; \
               getprop ro.build.version.sdk; getprop ro.build.id; \
               getprop ro.board.platform";

    let Ok(out) = adb.shell(serial, cmd).await else {
        return DeviceProperties::default();
    };

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

    DeviceProperties {
        friendly_name,
        brand: get(1),
        model: get(2),
        device_codename: get(3),
        manufacturer: get(4),
        android_release: get(5),
        sdk_level: get(6),
        build_id: get(7),
        board_platform: get(8),
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
}
