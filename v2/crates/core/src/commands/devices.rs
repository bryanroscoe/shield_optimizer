//! Device-list and profile commands.

use serde::Serialize;
use tauri::State;

use crate::adb::{
    batch_command, instance_serial, parse_device_list, parse_mdns_services, split_batch, AdbDriver,
    MdnsService, MDNS_SERVICE_CONNECT,
};
use crate::engine::{
    detect_device_type, tv_evidence,
    types::{ConnectionType, Device, DeviceProperties, DeviceStatus},
    DeviceType, TvEvidence,
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
                // Nothing was readable, so it told us nothing. The UI says
                // nothing about it in turn.
                tv_evidence: TvEvidence::Unknown,
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
            tv_evidence: tv_evidence(&props),
            status: e.status,
            connection: e.connection,
            properties: Some(props),
        });
    }

    Ok(renumber(collapse_duplicate_transports(out)))
}

/// `ro.serialno`, or `None` when the device gave us nothing to identify it by.
///
/// Empty and the literal `"unknown"` are both "no evidence" — some builds
/// report the latter rather than leaving the prop unset, and treating it as an
/// identity would merge every such device into one row.
fn hardware_id(device: &Device) -> Option<&str> {
    verified_hardware_id(&device.properties.as_ref()?.serial_number)
}

fn verified_hardware_id(raw: &str) -> Option<&str> {
    let id = raw.trim();
    if id.is_empty() || id.eq_ignore_ascii_case("unknown") {
        return None;
    }
    Some(id)
}

/// Collapse rows that are the same physical device reached two ways.
///
/// adb happily holds two transports for one device — it auto-connects a paired
/// mDNS device under its service name, and anything that also dials the same
/// device by `host:port` gets a second transport under a different key. Both
/// are real and both work; they are simply not two devices.
///
/// Identity is the verified hardware id and nothing else. Two rows are never
/// merged because their addresses look related — that is the repo-wide rule
/// the mobile app states as "matched on verified hardware id and never on IP
/// address alone", and it is why an unauthorized device (which cannot be
/// queried, so has no id) always keeps its own row.
///
/// The surviving row keeps the `ip:port` serial when one of the pair has it:
/// it is readable, it is what a user would type into Connect IP, and `adb -s`
/// accepts it just as well as the service name.
fn collapse_duplicate_transports(devices: Vec<Device>) -> Vec<Device> {
    let mut out: Vec<Device> = Vec::with_capacity(devices.len());

    for device in devices {
        let Some(id) = hardware_id(&device) else {
            // No evidence of identity: it stands alone.
            out.push(device);
            continue;
        };
        let existing = out
            .iter_mut()
            .find(|kept| hardware_id(kept) == Some(id) && kept.status == device.status);
        match existing {
            Some(kept) => {
                // Prefer the address a person can act on. `is_ip_port` rather
                // than "not ._tcp" so a USB serial never displaces one.
                if !is_ip_port(&kept.serial) && is_ip_port(&device.serial) {
                    kept.serial = device.serial;
                    kept.connection = device.connection;
                }
            }
            None => out.push(device),
        }
    }

    out
}

fn is_ip_port(serial: &str) -> bool {
    crate::adb::is_network_endpoint(serial)
}

/// `id` is a 1-based menu index, so it has to stay contiguous after a merge.
fn renumber(mut devices: Vec<Device>) -> Vec<Device> {
    for (idx, device) in devices.iter_mut().enumerate() {
        device.id = (idx + 1) as u32;
    }
    devices
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

pub async fn connect_device_impl(state: &AppState, address: &str) -> Result<ConnectResult, String> {
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

#[derive(Serialize, Debug)]
pub struct ForgetResult {
    pub ok: bool,
    /// Every transport key that was dropped, the requested one first.
    pub disconnected: Vec<String>,
    /// Is the device still advertising a Wireless debugging connect service?
    /// adb re-attaches a paired device by itself while it does, so the row is
    /// expected to come back within seconds.
    pub still_advertised: bool,
    pub message: String,
}

/// `forget_device` — drop every transport adb holds for this device.
///
/// adb keeps one transport per key, and a Wireless debugging device is often
/// held under two: the mDNS service name adb auto-connected, and an `ip:port`
/// someone dialled. The device list shows one row for both, so disconnecting
/// only the key that row displays left the other behind and the row stayed.
/// Aliases are matched on verified `ro.serialno` only; a transport whose id
/// cannot be read is never assumed to be the same device.
#[tauri::command]
pub async fn forget_device(
    state: State<'_, AppState>,
    serial: String,
) -> Result<ForgetResult, String> {
    forget_device_impl(state.inner(), &serial).await
}

pub async fn forget_device_impl(state: &AppState, serial: &str) -> Result<ForgetResult, String> {
    let adb = state.adb_snapshot().await;
    let raw = adb
        .raw(&["devices"])
        .await
        .map_err(|e| format!("adb devices: {e}"))?;
    let entries = parse_device_list(&raw.stdout);

    let mut targets = vec![serial.to_string()];
    let target_ready = entries
        .iter()
        .any(|e| e.serial == serial && e.status == DeviceStatus::Device);
    if target_ready {
        if let Some(id) = read_hardware_id(&*adb, serial).await {
            for entry in &entries {
                if entry.serial == serial
                    || entry.status != DeviceStatus::Device
                    || entry.connection != ConnectionType::Network
                {
                    continue;
                }
                if read_hardware_id(&*adb, &entry.serial).await.as_deref() == Some(id.as_str()) {
                    targets.push(entry.serial.clone());
                }
            }
        }
    }

    let mut disconnected = Vec::new();
    let mut failures = Vec::new();
    for target in &targets {
        state.drop_remote_session(target).await;
        match adb.raw(&["disconnect", target]).await {
            Ok(out) if out.success() && !out.combined().to_lowercase().contains("error") => {
                disconnected.push(target.clone())
            }
            Ok(out) => failures.push(format!("{target}: {}", out.combined().trim())),
            Err(e) => failures.push(format!("{target}: {e}")),
        }
    }

    let services = match adb.raw(&["mdns", "services"]).await {
        Ok(out) => parse_mdns_services(&out.stdout),
        Err(_) => Vec::new(),
    };
    let still_advertised = still_advertised(&services, &disconnected);

    let ok = failures.is_empty();
    let message = if !ok {
        format!("Could not disconnect {}", failures.join("; "))
    } else if still_advertised {
        READVERTISE_MESSAGE.to_string()
    } else if disconnected.len() > 1 {
        format!(
            "Disconnected {} connections to the same device.",
            disconnected.len()
        )
    } else {
        format!("Disconnected {serial}.")
    };

    Ok(ForgetResult {
        ok,
        disconnected,
        still_advertised,
        message,
    })
}

pub const READVERTISE_MESSAGE: &str = "Disconnected, but the device is still advertising \
    Wireless debugging, so adb will reconnect it by itself within a few seconds. To keep it off \
    this list, turn off Wireless debugging on the device, or remove this computer under Wireless \
    debugging \u{2192} Paired devices there.";

async fn read_hardware_id(adb: &dyn AdbDriver, serial: &str) -> Option<String> {
    let out = adb.shell(serial, "getprop ro.serialno").await.ok()?;
    verified_hardware_id(&out.stdout).map(str::to_string)
}

/// Would adb re-attach one of these transports by itself? It auto-connects
/// `_adb-tls-connect._tcp` services it holds a pairing for, keyed by
/// `<instance>._adb-tls-connect._tcp`. This compares adb's own transport keys,
/// not device identity.
fn still_advertised(services: &[MdnsService], disconnected: &[String]) -> bool {
    services
        .iter()
        .filter(|s| s.service == MDNS_SERVICE_CONNECT)
        .any(|s| {
            let prefix = format!("{}.", s.instance);
            let endpoint = s.endpoint();
            disconnected
                .iter()
                .any(|t| t.starts_with(&prefix) || *t == endpoint)
        })
}

/// What one look for a just-paired device's connect endpoint found.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PairedConnectProbe {
    /// Nothing advertised for that device yet; keep waiting.
    Waiting,
    /// We never learned which mDNS service the paired device is, so nothing
    /// advertised can be tied to it. The UI falls back to a typed address.
    Unidentified,
    /// adb already attached it under this key, so there is nothing to dial.
    Attached { serial: String },
    /// The single connect endpoint the paired device advertises.
    Endpoint { address: String },
    /// That device advertises more than one connect endpoint. Not guessing.
    Ambiguous { addresses: Vec<String> },
    /// The matching transport answered with a different `ro.serialno`, or
    /// none. Not accepted; the UI falls back to a typed address.
    NotThePairedDevice { message: String },
}

/// `probe_paired_connect` — one read of `adb mdns services` for the connect
/// endpoint a just-paired device advertises. The UI polls it: Android often
/// does not advertise, or accept, the connect service while its pairing dialog
/// is still open.
///
/// The device is identified by `instance`, the mDNS service instance its
/// pairing service carried when it was paired (see `pair_device`). The
/// pairing and connect instances differ in their random suffix but embed the
/// same `ro.serialno` (`adb-<serial>-<suffix>`), so connect services are
/// matched on that serial. An address is never enough: a different device
/// that held the same DHCP address earlier can still be in adb's mDNS view.
/// Without a parseable serial the answer is `Unidentified`, and
/// `connect_paired` re-checks `ro.serialno` on whatever it dials.
///
/// `pair_address` is what the user paired against. Its port is the pairing
/// port and is never reused; only an advertised `_adb-tls-connect._tcp` port is
/// ever returned.
#[tauri::command]
pub async fn probe_paired_connect(
    state: State<'_, AppState>,
    pair_address: String,
    instance: Option<String>,
) -> Result<PairedConnectProbe, String> {
    probe_paired_connect_impl(state.inner(), &pair_address, instance.as_deref()).await
}

pub async fn probe_paired_connect_impl(
    state: &AppState,
    pair_address: &str,
    instance: Option<&str>,
) -> Result<PairedConnectProbe, String> {
    let target = normalize_pairing_address(pair_address)?;
    let Some(instance) = instance.filter(|i| !i.trim().is_empty()) else {
        return Ok(PairedConnectProbe::Unidentified);
    };
    let adb = state.adb_snapshot().await;
    let services = read_mdns_services(&*adb).await;
    let attached: Vec<String> = match adb.raw(&["devices"]).await {
        Ok(out) => parse_device_list(&out.stdout)
            .into_iter()
            .filter(|e| e.status == DeviceStatus::Device)
            .map(|e| e.serial)
            .collect(),
        Err(_) => Vec::new(),
    };
    let probe = paired_connect_probe(&target, instance, &services, &attached);
    // A transport key is only a name. Before calling it the paired device,
    // ask it: a stale advertisement can point at an attached transport that
    // belongs to someone else.
    if let PairedConnectProbe::Attached { serial } = &probe {
        let expected = instance_serial(instance);
        let actual = read_hardware_id(&*adb, serial).await;
        if expected.is_none() || actual.as_deref() != expected {
            return Ok(PairedConnectProbe::NotThePairedDevice {
                message: format!(
                    "{serial} is attached, but it isn't the device that was just paired, so it \
                     wasn't picked."
                ),
            });
        }
    }
    Ok(probe)
}

async fn read_mdns_services(adb: &dyn AdbDriver) -> Vec<MdnsService> {
    match adb.raw(&["mdns", "services"]).await {
        Ok(out) => parse_mdns_services(&out.stdout),
        Err(_) => Vec::new(),
    }
}

fn same_host(advertised: &str, paired: &str) -> bool {
    let bare = |h: &str| {
        h.trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase()
    };
    bare(advertised) == bare(paired)
}

fn split_target(target: &str) -> Option<(&str, u16)> {
    let (host, port) = target.rsplit_once(':')?;
    Some((host, port.parse().ok()?))
}

/// The mDNS instance of the device being paired at `pair_target`: the one
/// `_adb-tls-pairing._tcp` service advertised on exactly that host *and*
/// pairing port. The pairing port is random per dialog, so this is the device
/// whose dialog the user is reading, not whoever else used the address. More
/// than one candidate, or none, is `None`.
fn pairing_instance(pair_target: &str, services: &[MdnsService]) -> Option<String> {
    let (host, port) = split_target(pair_target)?;
    if let Some((instance, _)) = host.split_once("._") {
        return Some(instance.to_string());
    }
    let mut found: Vec<&str> = services
        .iter()
        .filter(|s| s.is_pairing() && s.port == port && same_host(&s.host, host))
        .map(|s| s.instance.as_str())
        .collect();
    found.dedup();
    match found.as_slice() {
        [one] => Some((*one).to_string()),
        _ => None,
    }
}

fn paired_connect_probe(
    pair_target: &str,
    instance: &str,
    services: &[MdnsService],
    attached: &[String],
) -> PairedConnectProbe {
    let Some((_, pair_port)) = split_target(pair_target) else {
        return PairedConnectProbe::Waiting;
    };
    // Pairing and connect services carry different random suffixes; the
    // hardware serial between them is what they share.
    let Some(serial) = instance_serial(instance) else {
        return PairedConnectProbe::Unidentified;
    };

    let mut matches: Vec<&MdnsService> = Vec::new();
    for s in services
        .iter()
        .filter(|s| s.service == MDNS_SERVICE_CONNECT && s.instance_serial() == Some(serial))
        .filter(|s| s.port != pair_port)
    {
        if !matches.iter().any(|m| m.endpoint() == s.endpoint()) {
            matches.push(s);
        }
    }

    match matches.as_slice() {
        [] => PairedConnectProbe::Waiting,
        [service] => {
            let prefix = format!("{}.", service.instance);
            let endpoint = service.endpoint();
            match attached
                .iter()
                .find(|serial| serial.starts_with(&prefix) || **serial == endpoint)
            {
                Some(serial) => PairedConnectProbe::Attached {
                    serial: serial.clone(),
                },
                None => PairedConnectProbe::Endpoint { address: endpoint },
            }
        }
        many => PairedConnectProbe::Ambiguous {
            addresses: many.iter().map(|s| s.endpoint()).collect(),
        },
    }
}

#[derive(Serialize, Debug)]
pub struct PairedConnectResult {
    pub ok: bool,
    /// The endpoint answered but is not the device that was paired (or could
    /// not be confirmed as it). It has been disconnected; retrying the same
    /// advertisement will not help, so the UI stops and asks.
    pub not_the_paired_device: bool,
    pub message: String,
}

/// `connect_paired` — `adb connect` to the endpoint `probe_paired_connect`
/// found, then confirm it is the device that was paired: its `ro.serialno`
/// must equal the serial in the pairing `instance`. Anything else is
/// disconnected again and reported, so the UI falls back to the typed address
/// instead of showing a different device as the new one.
#[tauri::command]
pub async fn connect_paired(
    state: State<'_, AppState>,
    address: String,
    instance: String,
) -> Result<PairedConnectResult, String> {
    connect_paired_impl(state.inner(), &address, &instance).await
}

pub async fn connect_paired_impl(
    state: &AppState,
    address: &str,
    instance: &str,
) -> Result<PairedConnectResult, String> {
    let Some(expected) = instance_serial(instance) else {
        return Ok(PairedConnectResult {
            ok: false,
            not_the_paired_device: true,
            message: "The paired device's identity is unknown, so it wasn't connected \
                      automatically."
                .to_string(),
        });
    };
    let connected = connect_device_impl(state, address).await?;
    if !connected.ok {
        return Ok(PairedConnectResult {
            ok: false,
            not_the_paired_device: false,
            message: connected.message,
        });
    }
    let target = normalize_connect_address(address)?;
    let adb = state.adb_snapshot().await;
    let actual = read_hardware_id(&*adb, &target).await;
    if actual.as_deref() == Some(expected) {
        return Ok(PairedConnectResult {
            ok: true,
            not_the_paired_device: false,
            message: connected.message,
        });
    }
    let _ = adb.raw(&["disconnect", &target]).await;
    Ok(PairedConnectResult {
        ok: false,
        not_the_paired_device: true,
        message: match actual {
            Some(other) => format!(
                "{target} is a different device (serial {other}, expected {expected}), so it \
                 was disconnected."
            ),
            None => format!(
                "Couldn't confirm {target} is the device that was just paired, so it was \
                 disconnected."
            ),
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
) -> Result<PairResult, String> {
    pair_device_impl(state.inner(), &pair_address, &pin).await
}

#[derive(Serialize, Debug)]
pub struct PairResult {
    pub ok: bool,
    pub message: String,
    /// The mDNS instance of the device that was paired, read from its
    /// pairing service. `probe_paired_connect` finds the connect service by
    /// it. `None` when adb's mDNS view did not show that pairing service.
    pub instance: Option<String>,
}

async fn pair_device_impl(
    state: &AppState,
    pair_address: &str,
    pin: &str,
) -> Result<PairResult, String> {
    if let Err(message) = validate_pairing_pin(pin) {
        return Ok(PairResult {
            ok: false,
            message,
            instance: None,
        });
    }
    let target = normalize_pairing_address(pair_address)?;
    let adb = state.adb_snapshot().await;
    // Read before pairing: the device can stop advertising its pairing
    // service the moment pairing completes.
    let mut instance = pairing_instance(&target, &read_mdns_services(&*adb).await);
    let pair_out = adb
        .raw(&["pair", &target, pin])
        .await
        .map_err(|e| format!("adb pair: {e}"))?;
    let combined = pair_out.combined().trim().to_string();
    if !combined.to_lowercase().contains("successfully paired") {
        return Ok(PairResult {
            ok: false,
            message: combined,
            instance: None,
        });
    }
    if instance.is_none() {
        instance = pairing_instance(&target, &read_mdns_services(&*adb).await);
    }

    Ok(PairResult {
        ok: true,
        message: "Paired.".to_string(),
        instance,
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

/// Validate and normalize an endpoint for `adb connect` / `adb pair`.
///
/// Accepts the three shapes a device can actually be reached at:
/// - `IPv4[:port]` — the common case; a bare IP defaults to 5555, which is
///   right for legacy network debugging and wrong for Android 11+. Discovery
///   supplies the real port, so this default is a last resort for hand-typed
///   input rather than something the app relies on.
/// - `[IPv6]:port` — bracketed, as adb and every URL parser expect.
/// - `adb-XXXX-YYYY._adb-tls-connect._tcp[:port]` — an mDNS service name.
///   The daemon resolves these itself; rejecting them meant a user could not
///   paste what discovery had just shown them (GitHub #88).
///
/// Rejects empty input and any port that is not a positive 16-bit number.
pub fn normalize_connect_address(address: &str) -> Result<String, String> {
    let address = address.trim();
    if address.is_empty() {
        return Err("address is empty".to_string());
    }

    // A bracketed IPv6 literal carries colons of its own, so the port is what
    // follows the closing bracket, not the first colon.
    let (host, port) = if let Some(rest) = address.strip_prefix('[') {
        match rest.split_once(']') {
            Some((inner, "")) => (format!("[{inner}]"), "5555"),
            Some((inner, tail)) => match tail.strip_prefix(':') {
                Some(port) => (format!("[{inner}]"), port),
                None => return Err(format!("expected [IPv6]:port, got {address}")),
            },
            None => return Err(format!("unterminated IPv6 address: {address}")),
        }
    } else {
        match address.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p),
            None => (address.to_string(), "5555"),
        }
    };

    if host.is_empty() {
        return Err("address is missing a host".to_string());
    }
    if !is_ipv4(&host) && !is_bracketed_ipv6(&host) && !is_mdns_instance(&host) {
        return Err(format!(
            "not an IP address or mDNS service name: {host}. Enter the IP and port shown on \
             the TV's Wireless debugging screen."
        ));
    }

    match port.parse::<u16>() {
        Ok(0) => Err(format!("port must be 1-65535, got {port}")),
        Ok(_) => Ok(format!("{host}:{port}")),
        Err(_) => Err(format!("invalid port: {port}")),
    }
}

fn is_ipv4(host: &str) -> bool {
    let octets: Vec<&str> = host.split('.').collect();
    octets.len() == 4 && octets.iter().all(|o| o.parse::<u8>().is_ok())
}

/// Only the bracketed form. Bare IPv6 is ambiguous with `host:port` and adb
/// wants brackets anyway, so requiring them keeps the parse unambiguous.
fn is_bracketed_ipv6(host: &str) -> bool {
    // Validate exactly as `is_network_endpoint` does, so anything the parser
    // calls a network transport (including a scoped `fe80::1%en0`) can be
    // connected to.
    host.strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .is_some_and(|inner| {
            !inner.is_empty() && crate::adb::is_network_endpoint(&format!("[{inner}]:1"))
        })
}

/// An adb wireless-debugging mDNS instance, e.g.
/// `adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp`.
fn is_mdns_instance(host: &str) -> bool {
    host.contains("._tcp") && host.starts_with("adb-")
}

/// Batch-query device properties in a single shell call (matches v1's
/// optimization). The exact prop set is the union of what v1 used in
/// `Get-Devices` and `Show-DeviceProfile`.
///
/// Each read is its own sentinel-delimited section rather than one line of a
/// combined stdout. Positional parsing looked equivalent but was not: the
/// leading `settings get global device_name` can print nothing at all on some
/// builds and a multi-line Exception on others, and either one shifts every
/// later index — so a device would silently report its model as its Android
/// version. Sections cannot drift, and a read that produces nothing degrades
/// to an empty value instead of corrupting its neighbours.
async fn harvest_properties(adb: &dyn AdbDriver, serial: &str) -> Result<DeviceProperties, String> {
    let cmd = property_batch_command();

    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("device profile: {e}"))?;
    // Sections that did not print degrade to empty values on their own, so
    // the only real failure is a shell that produced nothing at all.
    if out.stdout.trim().is_empty() {
        return Err(format!(
            "device profile unavailable: {}",
            out.combined().trim()
        ));
    }

    Ok(properties_from_sections(&split_batch(
        &out.stdout,
        PROPERTY_READS.len(),
    )))
}

/// A batch reports only its LAST command's exit code, and the last read is
/// `pm has-feature`, which exits 1 to say "no" — an answer, not a failure. The
/// desktop driver turns any nonzero exit into an error before stdout is
/// looked at, so a phone that answered every question was refused. The
/// trailing `true` makes the batch's status say only that the shell ran.
fn property_batch_command() -> String {
    format!("{}; true", batch_command(&PROPERTY_READS))
}

/// The property reads, in the order `properties_from_sections` consumes them.
const PROPERTY_READS: [&str; 12] = [
    "settings get global device_name",
    "getprop ro.product.brand",
    "getprop ro.product.model",
    "getprop ro.product.device",
    "getprop ro.product.manufacturer",
    "getprop ro.build.version.release",
    "getprop ro.build.version.sdk",
    "getprop ro.build.id",
    "getprop ro.board.platform",
    "getprop ro.build.characteristics",
    "getprop ro.serialno",
    // The platform's own answer to "is this a TV?", independent of whatever
    // the OEM chose to put in ro.build.characteristics. Prints `true` or
    // `false`; anything else (an old build without the subcommand, a denied
    // shell) is no answer at all.
    "pm has-feature android.software.leanback",
];

/// Pure: map batched sections onto `DeviceProperties`. Split out so the
/// section-to-field mapping is testable without a driver.
fn properties_from_sections(sections: &[String]) -> DeviceProperties {
    let get = |i: usize| -> String {
        sections
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
        characteristics: get(9),
        serial_number: get(10),
        leanback: parse_bool_answer(&get(11)),
    }
}

/// `pm has-feature` prints `true` or `false`. Anything else — an empty
/// section, a usage message from a build that predates the subcommand, a
/// permission error — is not a "no": it is no answer, and the caller must not
/// read it as one.
fn parse_bool_answer(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
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

    /// Build what a device's batched property read looks like on the wire:
    /// one section per entry in `PROPERTY_READS`, sentinel-delimited. Short
    /// inputs pad with empty sections, mirroring a device that has a prop unset.
    fn batched_props(values: &[&str]) -> String {
        let mut sections: Vec<String> = values.iter().map(|v| (*v).to_string()).collect();
        sections.resize(PROPERTY_READS.len(), String::new());
        sections.join(&format!("\n{}\n", crate::adb::BATCH_SEPARATOR))
    }

    #[test]
    fn every_property_read_maps_to_a_field() {
        // The section-to-field mapping in `properties_from_sections` is
        // positional over PROPERTY_READS; a read added without a matching
        // `get(N)` would silently go nowhere.
        let sections: Vec<String> = (0..PROPERTY_READS.len())
            .map(|i| format!("value{i}"))
            .collect();
        let props = properties_from_sections(&sections);
        assert_eq!(props.friendly_name.as_deref(), Some("value0"));
        assert_eq!(props.brand, "value1");
        assert_eq!(props.model, "value2");
        assert_eq!(props.device_codename, "value3");
        assert_eq!(props.manufacturer, "value4");
        assert_eq!(props.android_release, "value5");
        assert_eq!(props.sdk_level, "value6");
        assert_eq!(props.build_id, "value7");
        assert_eq!(props.board_platform, "value8");
        assert_eq!(props.characteristics, "value9");
        assert_eq!(props.serial_number, "value10");
        // `value11` is neither `true` nor `false`, so the leanback answer is
        // absent rather than negative — the field is still fed by section 11.
        assert_eq!(props.leanback, None);
        let yes = vec!["true".to_string(); PROPERTY_READS.len()];
        assert_eq!(properties_from_sections(&yes).leanback, Some(true));
    }

    #[cfg(unix)]
    #[test]
    fn the_property_batch_exits_zero_when_the_last_read_says_no() {
        // Stand-ins that behave like the real reads: `pm has-feature` prints
        // `false` and exits 1. The real driver rejects any nonzero status, so
        // the batch as a whole must not carry that 1.
        let script = format!(
            "settings() {{ echo x; }}; getprop() {{ echo x; }}; pm() {{ echo false; return 1; }}; {}",
            property_batch_command()
        );
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output()
            .unwrap();
        assert!(out.status.success(), "{:?}", out.status);
        let sections = split_batch(&String::from_utf8_lossy(&out.stdout), PROPERTY_READS.len());
        assert_eq!(properties_from_sections(&sections).leanback, Some(false));
    }

    #[tokio::test]
    async fn a_device_that_answers_no_to_leanback_is_still_profiled() {
        // The live regression: a batch reports only its last command's exit
        // code, and the last read is `pm has-feature`, which exits 1 to say
        // "no". A Pixel that answered every question was refused with
        // "adb process failed (exit code Some(1))". The exit code carries no
        // information about the reads that came before it.
        let mock = MockAdb::default().on_shell_exit(
            "settings get global device_name",
            &batched_props(&[
                "Bryan's Pixel",
                "google",
                "Pixel 8",
                "shiba",
                "Google",
                "15",
                "35",
                "AP4A",
                "zuma",
                "",
                "58040DLCH005YV",
                "false",
            ]),
            1,
        );

        let props = harvest_properties(&mock, "58040DLCH005YV").await.unwrap();

        assert_eq!(props.leanback, Some(false));
        assert_eq!(props.brand, "google");
        assert_eq!(props.model, "Pixel 8");
        assert_eq!(props.serial_number, "58040DLCH005YV");
    }

    #[tokio::test]
    async fn a_shell_that_prints_nothing_is_still_a_failure() {
        let mock = MockAdb::default().on_shell_exit("settings get global device_name", "", 1);

        let err = harvest_properties(&mock, "58040DLCH005YV")
            .await
            .unwrap_err();

        assert!(err.contains("device profile unavailable"), "{err}");
    }

    #[tokio::test]
    async fn silent_device_name_read_does_not_shift_the_android_version() {
        // `settings get global device_name` prints nothing on some builds.
        // Under the old positional parse every later value slid up one, so the
        // TV reported its brand as its friendly name and its model as its
        // Android version. Sections keep each read in its own slot.
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                "List of devices attached\n192.168.42.71:5555\tdevice\n",
            )
            .on_shell(
                "settings get global device_name",
                &batched_props(&[
                    "",
                    "NVIDIA",
                    "SHIELD Android TV",
                    "mdarcy",
                    "NVIDIA",
                    "11",
                    "30",
                    "PPR1",
                    "tegra",
                    "tv",
                    "0323220012345",
                ]),
            );
        let state = state_with(mock);
        let devices = list_devices_impl(&state).await.unwrap();
        let props = devices[0].properties.as_ref().unwrap();

        assert_eq!(props.friendly_name, None);
        assert_eq!(props.brand, "NVIDIA");
        assert_eq!(props.android_release, "11");
        assert_eq!(props.sdk_level, "30");
        assert_eq!(props.serial_number, "0323220012345");
    }

    #[tokio::test]
    async fn multiline_settings_exception_stays_inside_its_own_section() {
        // The other failure shape: `settings get` throws and prints a
        // multi-line stack trace, which under positional parsing pushed every
        // real property down by however many lines the trace happened to be.
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                "List of devices attached\n192.168.42.71:5555\tdevice\n",
            )
            .on_shell(
                "settings get global device_name",
                &batched_props(&[
                    "Exception occurred while executing:\n  java.lang.SecurityException\n  at android.os.Parcel",
                    "TCL",
                    "QM7L Pro",
                    "",
                    "TCL",
                    "14",
                    "34",
                ]),
            );
        let state = state_with(mock);
        let devices = list_devices_impl(&state).await.unwrap();
        let props = devices[0].properties.as_ref().unwrap();

        assert_eq!(props.friendly_name, None);
        assert_eq!(props.brand, "TCL");
        assert_eq!(props.model, "QM7L Pro");
        assert_eq!(props.android_release, "14");
        assert_eq!(props.sdk_level, "34");
        // Reads the device never answered stay empty rather than borrowing a
        // neighbour's value.
        assert_eq!(props.build_id, "");
        assert_eq!(props.serial_number, "");
    }

    fn row(serial: &str, hardware_id: &str) -> Device {
        Device {
            id: 0,
            serial: serial.to_string(),
            name: "TV".into(),
            model: "TV".into(),
            device_type: DeviceType::Unknown,
            tv_evidence: TvEvidence::Unknown,
            status: DeviceStatus::Device,
            connection: crate::engine::types::ConnectionType::Network,
            properties: Some(DeviceProperties {
                serial_number: hardware_id.to_string(),
                ..Default::default()
            }),
        }
    }

    fn unidentified(serial: &str) -> Device {
        Device {
            id: 0,
            serial: serial.to_string(),
            name: serial.to_string(),
            model: String::new(),
            device_type: DeviceType::Unknown,
            tv_evidence: TvEvidence::Unknown,
            status: DeviceStatus::Unauthorized,
            connection: crate::engine::types::ConnectionType::Network,
            properties: None,
        }
    }

    #[test]
    fn one_device_reached_two_ways_collapses_to_the_usable_address() {
        // The live regression: adb auto-connected the device under its mDNS
        // service name, and the scan then dialled the same device by address.
        let devices = vec![
            row(
                "adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp",
                "58040DLCH005YV",
            ),
            row("192.168.42.211:34083", "58040DLCH005YV"),
        ];

        let collapsed = renumber(collapse_duplicate_transports(devices));

        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].serial, "192.168.42.211:34083");
        assert_eq!(collapsed[0].id, 1);
    }

    #[test]
    fn the_service_name_survives_when_it_is_the_only_transport() {
        let devices = vec![row(
            "adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp",
            "58040DLCH005YV",
        )];

        let collapsed = collapse_duplicate_transports(devices);

        assert_eq!(collapsed.len(), 1);
        assert_eq!(
            collapsed[0].serial,
            "adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp"
        );
    }

    #[test]
    fn a_usb_serial_is_never_displaced_by_an_address() {
        // A device on USB and network at once keeps the USB row's serial only
        // if the address form does not exist; here it does, and the address is
        // the one a person can retype. The point of the assertion is that the
        // *USB* serial is not mistaken for an address by is_ip_port.
        assert!(!is_ip_port("0323220012345"));
        assert!(!is_ip_port(
            "adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp"
        ));
        assert!(is_ip_port("192.168.42.211:34083"));
        assert!(is_ip_port("[fe80::1]:41541"));
        assert!(!is_ip_port("192.168.42.211"));
        assert!(!is_ip_port("192.168.42:5555"));
    }

    #[test]
    fn different_hardware_ids_stay_separate_however_alike_the_addresses_look() {
        let devices = vec![
            row("192.168.42.211:34083", "AAAAAAA"),
            row("192.168.42.211:5555", "BBBBBBB"),
        ];

        let collapsed = collapse_duplicate_transports(devices);

        assert_eq!(collapsed.len(), 2, "two TVs at one address are two TVs");
    }

    #[test]
    fn devices_without_an_id_are_never_merged_into_each_other() {
        // Unauthorized devices cannot be queried, so there is no evidence they
        // are the same device. Merging on address alone is exactly the claim
        // the repo's identity rule forbids.
        let devices = vec![
            unidentified("192.168.42.143:5555"),
            unidentified("adb-mystery._adb-tls-connect._tcp"),
        ];

        let collapsed = collapse_duplicate_transports(devices);

        assert_eq!(collapsed.len(), 2);
    }

    #[test]
    fn an_identified_device_never_absorbs_an_unidentified_one() {
        let devices = vec![
            row("192.168.42.211:34083", "58040DLCH005YV"),
            unidentified("adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp"),
        ];

        let collapsed = collapse_duplicate_transports(devices);

        assert_eq!(
            collapsed.len(),
            2,
            "an unauthorized row has no verified id, so it cannot be merged"
        );
    }

    #[test]
    fn placeholder_hardware_ids_are_not_identities() {
        // Some builds answer `unknown` rather than leaving ro.serialno unset.
        // Treating that as an id would fold every such device into one row.
        let devices = vec![
            row("192.168.42.1:5555", "unknown"),
            row("192.168.42.2:5555", "Unknown"),
            row("192.168.42.3:5555", "   "),
        ];

        let collapsed = collapse_duplicate_transports(devices);

        assert_eq!(collapsed.len(), 3);
        assert!(collapsed.iter().all(|d| hardware_id(d).is_none()));
    }

    #[test]
    fn a_disconnected_twin_does_not_merge_with_a_live_one() {
        // Same hardware, but one transport is offline. Collapsing them would
        // report a dead endpoint as reachable.
        let mut offline = row("192.168.42.211:5555", "58040DLCH005YV");
        offline.status = DeviceStatus::Offline;
        let devices = vec![row("192.168.42.211:34083", "58040DLCH005YV"), offline];

        let collapsed = collapse_duplicate_transports(devices);

        assert_eq!(collapsed.len(), 2);
    }

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
            // harvest_properties' batched reads — give a brand so the name
            // resolves; other props default.
            .on_shell(
                "settings get global device_name",
                &batched_props(&["Living Room", "NVIDIA"]),
            );
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

    #[test]
    fn normalize_accepts_a_scoped_ipv6_endpoint_from_discovery() {
        // Codex on #128: mDNS can report a link-local host with its scope,
        // and endpoint() brackets it as-is. Rejecting it here meant the
        // auto-connect retried for 45 s and gave up on a device it had found.
        assert_eq!(
            normalize_connect_address("[fe80::1%en0]:41541").unwrap(),
            "[fe80::1%en0]:41541"
        );
        assert_eq!(
            normalize_connect_address("[fe80::1]:41541").unwrap(),
            "[fe80::1]:41541"
        );
        assert!(normalize_connect_address("[fe80::zz%en0]:41541").is_err());
        let services = parse_mdns_services(
            "List of discovered mdns services\n\
             adb-V6SER-c2\t_adb-tls-connect._tcp\tfe80::1%en0:41541\n",
        );
        assert!(normalize_connect_address(&services[0].endpoint()).is_ok());
    }

    #[test]
    fn normalize_accepts_an_mdns_service_name_from_discovery() {
        // Discovery shows these; refusing to accept one back meant a user
        // could not paste what the app had just told them (GitHub #88).
        let instance = "adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp";
        assert_eq!(
            normalize_connect_address(&format!("{instance}:41541")).unwrap(),
            format!("{instance}:41541")
        );
        assert_eq!(
            normalize_connect_address(instance).unwrap(),
            format!("{instance}:5555")
        );
    }

    #[test]
    fn normalize_keeps_a_bracketed_ipv6_host_whole() {
        // The port is what follows the bracket, not the first colon.
        assert_eq!(
            normalize_connect_address("[fe80::1c2d:3e4f]:41541").unwrap(),
            "[fe80::1c2d:3e4f]:41541"
        );
        assert_eq!(normalize_connect_address("[::1]").unwrap(), "[::1]:5555");
    }

    #[test]
    fn normalize_still_rejects_things_that_are_not_endpoints() {
        for bad in [
            "not-a-host:5555",
            "192.168.1:5555",
            "192.168.1.300:5555",
            "[fe80::zz]:5555",
            "[fe80::1",
            "[fe80::1]5555",
            ":5555",
            "192.168.1.5:0",
            "192.168.1.5:port",
        ] {
            assert!(
                normalize_connect_address(bad).is_err(),
                "{bad} should not be accepted"
            );
        }
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
        assert_eq!(result.message, "Paired.");
        assert_eq!(result.instance, None, "no pairing service was advertised");
        let log = raw_log.lock().unwrap();
        assert!(log.contains(&"pair 192.168.42.71:43219 123456".to_string()));
        assert!(
            log.iter().all(|c| !c.starts_with("connect")),
            "pairing never connects by itself: {log:?}"
        );
    }

    #[tokio::test]
    async fn pair_records_the_instance_advertised_on_that_pairing_port() {
        let mock = MockAdb::default()
            .on_raw(
                "pair 192.168.42.211:45439 123456",
                "Successfully paired to 192.168.42.211:45439\n",
            )
            .on_raw("mdns services", &mdns_pixel(46545));
        let state = state_with(mock);

        let result = pair_device_impl(&state, "192.168.42.211:45439", "123456")
            .await
            .unwrap();

        assert_eq!(
            result.instance.as_deref(),
            Some("adb-58040DLCH005YV-A1b2C3")
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
        let log = raw_log.lock().unwrap();
        assert!(log.contains(&"pair 192.168.42.71:43219 123456".to_string()));
        assert!(log.iter().all(|c| !c.starts_with("connect")), "{log:?}");
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

    const PIXEL_MDNS: &str = "adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp";
    const PIXEL_IP: &str = "192.168.42.211:46545";

    fn mdns_pixel(port: u16) -> String {
        format!(
            "List of discovered mdns services\n\
             adb-58040DLCH005YV-jBeCEe\t_adb-tls-connect._tcp\t192.168.42.211:{port}\n\
             adb-58040DLCH005YV-A1b2C3\t_adb-tls-pairing._tcp\t192.168.42.211:45439\n\
             adb-0323716101827\t_adb._tcp\t192.168.42.71:5555\n"
        )
    }

    #[tokio::test]
    async fn forget_drops_every_transport_of_the_same_hardware() {
        // What the owner's Pixel looked like: adb held it under its mDNS name
        // *and* under ip:port. The row shows ip:port, so disconnecting only
        // that left the mDNS transport behind and the row stayed.
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                &format!(
                    "List of devices attached\n{PIXEL_IP}\tdevice\n{PIXEL_MDNS}\tdevice\n\
                     192.168.42.71:5555\tdevice\n192.168.42.143:5555\tunauthorized\n"
                ),
            )
            .on_raw("disconnect", "disconnected")
            .on_raw("mdns services", "List of discovered mdns services\n")
            .on_shell_for(PIXEL_IP, "ro.serialno", "58040DLCH005YV\n")
            .on_shell_for(PIXEL_MDNS, "ro.serialno", "58040DLCH005YV\n")
            .on_shell_for("192.168.42.71:5555", "ro.serialno", "0323716101827\n");
        let log = mock.raw_log();
        let state = state_with(mock);

        let r = forget_device_impl(&state, PIXEL_IP).await.unwrap();

        assert!(r.ok, "{r:?}");
        assert_eq!(
            r.disconnected,
            vec![PIXEL_IP.to_string(), PIXEL_MDNS.to_string()]
        );
        assert!(!r.still_advertised);
        let log = log.lock().unwrap();
        let disconnects: Vec<&String> =
            log.iter().filter(|c| c.starts_with("disconnect")).collect();
        assert_eq!(
            disconnects,
            vec![
                &format!("disconnect {PIXEL_IP}"),
                &format!("disconnect {PIXEL_MDNS}")
            ],
            "the Shield and the unauthorized box must be left alone"
        );
    }

    #[tokio::test]
    async fn forget_never_matches_an_alias_on_address_or_missing_id() {
        // Same host, different port, and an id that reads "unknown": that is
        // no evidence, so only the requested transport is dropped.
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                "List of devices attached\n192.168.42.211:46545\tdevice\n192.168.42.211:5555\tdevice\n",
            )
            .on_raw("disconnect", "disconnected")
            .on_shell_for("192.168.42.211:46545", "ro.serialno", "unknown\n")
            .on_shell_for("192.168.42.211:5555", "ro.serialno", "unknown\n");
        let state = state_with(mock);

        let r = forget_device_impl(&state, "192.168.42.211:46545")
            .await
            .unwrap();

        assert_eq!(r.disconnected, vec!["192.168.42.211:46545".to_string()]);
    }

    #[tokio::test]
    async fn forget_says_plainly_when_adb_will_reattach_it() {
        // Observed on the Pixel: adb's mDNS auto-connect re-attached it about
        // 9 s after `adb disconnect`, because Wireless debugging was still on.
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                &format!("List of devices attached\n{PIXEL_MDNS}\tdevice\n"),
            )
            .on_raw("disconnect", "disconnected")
            .on_raw("mdns services", &mdns_pixel(46545))
            .on_shell_for(PIXEL_MDNS, "ro.serialno", "58040DLCH005YV\n");
        let state = state_with(mock);

        let r = forget_device_impl(&state, PIXEL_MDNS).await.unwrap();

        assert!(r.ok);
        assert!(r.still_advertised);
        assert_eq!(r.message, READVERTISE_MESSAGE);
        assert!(r.message.contains("turn off Wireless debugging"));
    }

    #[tokio::test]
    async fn forget_reports_a_disconnect_adb_refused() {
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                "List of devices attached\n192.168.42.9:5555\toffline\n",
            )
            .on_raw("disconnect", "error: no such device '192.168.42.9:5555'");
        let state = state_with(mock);

        let r = forget_device_impl(&state, "192.168.42.9:5555")
            .await
            .unwrap();

        assert!(!r.ok);
        assert!(r.disconnected.is_empty());
        assert!(r.message.contains("no such device"), "{}", r.message);
    }

    fn svc(instance: &str, service: &str, host: &str, port: u16) -> MdnsService {
        MdnsService {
            instance: instance.into(),
            service: service.into(),
            host: host.into(),
            port,
        }
    }

    // The real platform-tools 37 pair from adb/parse.rs: one TV, two services,
    // different random suffixes, the same embedded serial.
    const PIXEL_PAIR: &str = "adb-58040DLCH005YV-A1b2C3";
    const PIXEL: &str = "adb-58040DLCH005YV-jBeCEe";

    #[test]
    fn probe_waits_until_the_connect_service_appears() {
        // Only the pairing service: the dialog is still open on the phone.
        let services = vec![svc(
            PIXEL_PAIR,
            crate::adb::MDNS_SERVICE_PAIRING,
            "192.168.42.211",
            45439,
        )];
        assert_eq!(
            paired_connect_probe("192.168.42.211:45439", PIXEL_PAIR, &services, &[]),
            PairedConnectProbe::Waiting
        );
    }

    #[test]
    fn probe_matches_the_connect_service_by_its_embedded_serial() {
        // Codex on #128: the instances differ in suffix, so exact equality
        // never auto-connected. The serial between them is the identity.
        let services = parse_mdns_services(&mdns_pixel(46545));
        assert_eq!(
            paired_connect_probe("192.168.42.211:45439", PIXEL_PAIR, &services, &[]),
            PairedConnectProbe::Endpoint {
                address: PIXEL_IP.to_string()
            }
        );
        let same_port = vec![svc(PIXEL, MDNS_SERVICE_CONNECT, "192.168.42.211", 45439)];
        assert_eq!(
            paired_connect_probe("192.168.42.211:45439", PIXEL_PAIR, &same_port, &[]),
            PairedConnectProbe::Waiting,
            "never the pairing port"
        );
    }

    #[test]
    fn probe_never_takes_another_device_on_the_same_address() {
        // A device that held this DHCP address earlier can still be in adb's
        // mDNS view before the new one advertises. Same IP, different serial.
        let services = vec![svc(
            "adb-OLDTV0001-xyz",
            MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            40111,
        )];
        assert_eq!(
            paired_connect_probe("192.168.42.211:45439", PIXEL_PAIR, &services, &[]),
            PairedConnectProbe::Waiting
        );
    }

    #[test]
    fn probe_picks_the_paired_serial_when_two_connect_services_share_an_ip() {
        let services = vec![
            svc(
                "adb-OLDTV0001-xyz",
                MDNS_SERVICE_CONNECT,
                "192.168.42.211",
                40111,
            ),
            svc(PIXEL, MDNS_SERVICE_CONNECT, "192.168.42.211", 46545),
        ];
        assert_eq!(
            paired_connect_probe("192.168.42.211:45439", PIXEL_PAIR, &services, &[]),
            PairedConnectProbe::Endpoint {
                address: PIXEL_IP.to_string()
            }
        );
    }

    #[test]
    fn probe_with_a_malformed_pairing_instance_claims_nothing() {
        let services = parse_mdns_services(&mdns_pixel(46545));
        for bad in [
            "adb-unknown-A1b2C3",
            "adb--A1b2C3",
            "Living Room TV",
            "adb-58040DLCH005YV",
        ] {
            assert_eq!(
                paired_connect_probe("192.168.42.211:45439", bad, &services, &[]),
                PairedConnectProbe::Unidentified,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn an_attached_transport_is_accepted_only_when_its_serial_matches() {
        let mock = MockAdb::default()
            .on_raw("mdns services", &mdns_pixel(46545))
            .on_raw(
                "devices",
                &format!("List of devices attached\n{PIXEL_MDNS}\tdevice\n"),
            )
            .on_shell_for(PIXEL_MDNS, "ro.serialno", "58040DLCH005YV\n");
        let state = state_with(mock);
        assert_eq!(
            probe_paired_connect_impl(&state, "192.168.42.211:45439", Some(PIXEL_PAIR))
                .await
                .unwrap(),
            PairedConnectProbe::Attached {
                serial: PIXEL_MDNS.to_string()
            }
        );
    }

    #[tokio::test]
    async fn a_stale_advertisement_pointing_at_another_attached_device_is_refused() {
        // Codex on #128: the advertised endpoint names a transport adb holds,
        // but that transport is a different device. Its key must not be
        // enough to call it the one just paired.
        let mock = MockAdb::default()
            .on_raw("mdns services", &mdns_pixel(46545))
            .on_raw(
                "devices",
                &format!("List of devices attached\n{PIXEL_IP}\tdevice\n"),
            )
            .on_shell_for(PIXEL_IP, "ro.serialno", "OLDTV0001\n");
        let state = state_with(mock);
        let probe = probe_paired_connect_impl(&state, "192.168.42.211:45439", Some(PIXEL_PAIR))
            .await
            .unwrap();
        assert!(
            matches!(probe, PairedConnectProbe::NotThePairedDevice { .. }),
            "{probe:?}"
        );
    }

    #[tokio::test]
    async fn probe_without_an_instance_claims_nothing() {
        let mock = MockAdb::default().on_raw("mdns services", &mdns_pixel(46545));
        let log = mock.raw_log();
        let state = state_with(mock);
        assert_eq!(
            probe_paired_connect_impl(&state, "192.168.42.211:45439", None)
                .await
                .unwrap(),
            PairedConnectProbe::Unidentified
        );
        assert!(
            log.lock().unwrap().is_empty(),
            "nothing to look up without an identity"
        );
    }

    #[test]
    fn pairing_instance_is_the_service_on_that_exact_pairing_port() {
        let services = vec![
            svc(
                "adb-OLDTV0001-xyz",
                MDNS_SERVICE_CONNECT,
                "192.168.42.211",
                40111,
            ),
            svc(
                "adb-OLDTV0001-xyz",
                crate::adb::MDNS_SERVICE_PAIRING,
                "192.168.42.211",
                39000,
            ),
            svc(
                PIXEL_PAIR,
                crate::adb::MDNS_SERVICE_PAIRING,
                "192.168.42.211",
                45439,
            ),
        ];
        assert_eq!(
            pairing_instance("192.168.42.211:45439", &services).as_deref(),
            Some(PIXEL_PAIR)
        );
        assert_eq!(pairing_instance("192.168.42.211:45440", &services), None);
        assert_eq!(
            pairing_instance("adb-V6SER-p1._adb-tls-pairing._tcp:37000", &[]).as_deref(),
            Some("adb-V6SER-p1")
        );
    }

    #[test]
    fn probe_reports_a_transport_adb_already_attached() {
        let services = parse_mdns_services(&mdns_pixel(46545));
        assert_eq!(
            paired_connect_probe(
                "192.168.42.211:45439",
                PIXEL_PAIR,
                &services,
                &[PIXEL_MDNS.to_string()]
            ),
            PairedConnectProbe::Attached {
                serial: PIXEL_MDNS.to_string()
            }
        );
    }

    #[test]
    fn probe_does_not_guess_between_two_connect_ports() {
        let services = vec![
            svc(PIXEL, MDNS_SERVICE_CONNECT, "192.168.42.211", 40001),
            svc(PIXEL, MDNS_SERVICE_CONNECT, "fe80::1", 40002),
        ];
        assert!(matches!(
            paired_connect_probe("192.168.42.211:45439", PIXEL_PAIR, &services, &[]),
            PairedConnectProbe::Ambiguous { addresses } if addresses.len() == 2
        ));
    }

    #[test]
    fn probe_brackets_an_ipv6_connect_endpoint_and_sees_it_attached() {
        let services = vec![svc("adb-V6SER-c2", MDNS_SERVICE_CONNECT, "fe80::1", 41541)];
        assert_eq!(
            paired_connect_probe("[fe80::1]:37000", "adb-V6SER-p1", &services, &[]),
            PairedConnectProbe::Endpoint {
                address: "[fe80::1]:41541".to_string()
            }
        );
        assert_eq!(
            paired_connect_probe(
                "[fe80::1]:37000",
                "adb-V6SER-p1",
                &services,
                &["[fe80::1]:41541".to_string()]
            ),
            PairedConnectProbe::Attached {
                serial: "[fe80::1]:41541".to_string()
            }
        );
    }

    #[test]
    fn a_bracketed_ipv6_transport_still_counts_as_advertised() {
        // Codex on #128: adb keys the transport `[fe80::1]:41541`, the
        // advertisement is printed bare. Compared raw, Forget stayed silent.
        let services = parse_mdns_services(
            "List of discovered mdns services\n\
             adb-V6SER-c2\t_adb-tls-connect._tcp\tfe80::1:41541\n",
        );
        assert!(still_advertised(
            &services,
            &["[fe80::1]:41541".to_string()]
        ));
    }

    #[tokio::test]
    async fn connect_paired_keeps_the_device_whose_serial_matches() {
        let mock = MockAdb::default()
            .on_raw("connect", &format!("connected to {PIXEL_IP}"))
            .on_shell_for(PIXEL_IP, "ro.serialno", "58040DLCH005YV\n");
        let log = mock.raw_log();
        let state = state_with(mock);
        let r = connect_paired_impl(&state, PIXEL_IP, PIXEL_PAIR)
            .await
            .unwrap();
        assert!(r.ok, "{}", r.message);
        assert!(log
            .lock()
            .unwrap()
            .iter()
            .all(|c| !c.starts_with("disconnect")));
    }

    #[tokio::test]
    async fn connect_paired_drops_a_different_device() {
        let mock = MockAdb::default()
            .on_raw("disconnect", "disconnected")
            .on_raw("connect", &format!("connected to {PIXEL_IP}"))
            .on_shell_for(PIXEL_IP, "ro.serialno", "OLDTV0001\n");
        let log = mock.raw_log();
        let state = state_with(mock);
        let r = connect_paired_impl(&state, PIXEL_IP, PIXEL_PAIR)
            .await
            .unwrap();
        assert!(!r.ok);
        assert!(r.not_the_paired_device);
        assert!(r.message.contains("different device"), "{}", r.message);
        assert!(log
            .lock()
            .unwrap()
            .contains(&format!("disconnect {PIXEL_IP}")));
    }

    #[tokio::test]
    async fn connect_paired_refuses_without_a_parseable_serial() {
        let mock = MockAdb::default();
        let log = mock.raw_log();
        let state = state_with(mock);
        let r = connect_paired_impl(&state, PIXEL_IP, "Living Room TV")
            .await
            .unwrap();
        assert!(!r.ok);
        assert!(log.lock().unwrap().is_empty(), "nothing dialled");
    }
}
