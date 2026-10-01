//! Network-scan command — matches v1's `Scan-Network` UX.

use std::time::Duration;

use serde::Serialize;
use tauri::State;

use crate::adb::{
    local_subnet_prefix, parse_device_list, parse_mdns_services, scan_subnet, AdbDriver,
    MdnsService, ADB_NETWORK_PORT,
};
use shield_optimizer_core::engine::types::DeviceStatus;

use super::AppState;

#[derive(Serialize)]
pub struct ScanResult {
    /// First three octets of the scanned /24 (e.g. "192.168.42"), or `null`
    /// if the gateway couldn't be detected.
    pub subnet: Option<String>,
    /// IPs that answered on the ADB port.
    pub found: Vec<String>,
    /// IPs that `adb connect` succeeded against.
    pub connected: Vec<String>,
    /// IPs the daemon reached but that haven't authorized this computer's ADB
    /// key — the device shows a debugging authorization prompt and registers
    /// as `unauthorized` in the device list.
    pub unauthorized: Vec<String>,
    /// IPs that responded to the port probe but `adb connect` failed.
    pub failed: Vec<String>,
    /// Devices advertising only an Android 11+ *pairing* service. These cannot
    /// be connected to until the user enters the 6-digit code from the TV, so
    /// they are reported separately rather than counted as failures. Each
    /// entry is the pairing `host:port` to type into Pair PIN.
    pub needs_pairing: Vec<String>,
    /// Human-readable summary line — useful diagnostic for the UI.
    pub message: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum ConnectOutcome {
    Connected,
    /// Device reachable but this host's ADB key isn't approved yet. The
    /// device is still added to `adb devices` (as `unauthorized`), so this
    /// is "waiting on the user", not a failure — and retrying won't help.
    Unauthorized,
    Failed,
}

/// Classify `adb connect <target>` output. Detection is text-based on the
/// combined streams rather than the exit code, since `adb connect` exits 0
/// even on "failed to authenticate" / "failed to connect" with current
/// platform-tools, and exit-code conventions vary across versions.
pub(crate) fn classify_connect_output(combined: &str) -> ConnectOutcome {
    let s = combined.to_lowercase();
    if s.contains("failed to authenticate") {
        ConnectOutcome::Unauthorized
    } else if s.contains("connected to") && !s.contains("failed") && !s.contains("cannot") {
        ConnectOutcome::Connected
    } else {
        ConnectOutcome::Failed
    }
}

/// A nonzero exit surfaces as `Err` from the driver and counts as failed.
async fn adb_connect(adb: &dyn AdbDriver, target: &str) -> ConnectOutcome {
    match adb.raw(&["connect", target]).await {
        Ok(out) => classify_connect_output(&format!("{}\n{}", out.stdout, out.stderr)),
        Err(_) => ConnectOutcome::Failed,
    }
}

/// Where the scan will try to connect, and what still needs pairing first.
struct ScanTargets {
    /// `host:port` endpoints to hand to `adb connect`, in a stable order.
    connect: Vec<String>,
    /// Advertised endpoints not dialled because adb already holds a ready
    /// transport for that service. They were found and are connected; they
    /// just need no second key.
    already_attached: Vec<String>,
    /// Pairing `host:port` for devices that advertise no connectable service.
    needs_pairing: Vec<String>,
}

/// Merge mDNS advertisements with the raw `:5555` port sweep.
///
/// Android 11+ wireless debugging listens on a random port that changes every
/// time it is toggled, so a `:5555` sweep cannot see it at all — which is what
/// "device not supported" actually meant in GitHub #88. mDNS knows the real
/// port, so when a host advertises one we use it and drop the swept `:5555`
/// guess for that same host. Legacy devices (Shield with Network debugging)
/// advertise `_adb._tcp` on 5555 or nothing at all, and keep working either way.
/// Would dialling this service hand adb a *second* key for a device it already
/// holds?
///
/// adb auto-connects an already-paired mDNS device by itself and keys that
/// transport by the *service name*, e.g.
/// `adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp`. The advertisement gives
/// us the instance, `adb-58040DLCH005YV-jBeCEe`, so the attached serial is the
/// instance plus a dot-prefixed service suffix. Connecting the same device
/// again by `host:port` registers a second transport under a different key,
/// which is how one device came to occupy two rows.
///
/// Deliberately *not* checked here: a device already attached under the very
/// endpoint we would dial. That reconnect is a no-op adb answers with "already
/// connected to …", it cannot produce a second transport, and skipping it
/// would drop the device out of the scan's connected count — telling a user
/// their TV was not found when it is sitting right there.
fn would_duplicate_transport(service: &MdnsService, attached: &[String]) -> bool {
    let instance_prefix = format!("{}.", service.instance);
    let endpoint = service.endpoint();
    attached
        .iter()
        .filter(|serial| *serial != &endpoint)
        .any(|serial| serial == &service.instance || serial.starts_with(&instance_prefix))
}

fn merge_scan_targets(
    swept_ips: &[String],
    services: &[MdnsService],
    attached: &[String],
) -> ScanTargets {
    let mut connect: Vec<String> = Vec::new();
    let mut already_attached: Vec<String> = Vec::new();
    let mut advertised_hosts: Vec<&str> = Vec::new();

    for service in services.iter().filter(|s| s.is_connectable()) {
        // The host still counts as spoken-for either way, so the `:5555` sweep
        // below does not guess at a device that told us its real port.
        if !advertised_hosts.contains(&service.host.as_str()) {
            advertised_hosts.push(&service.host);
        }
        // Reconnecting under a second key gains nothing and costs a duplicate
        // row.
        let endpoint = service.endpoint();
        if would_duplicate_transport(service, attached) {
            if !already_attached.contains(&endpoint) {
                already_attached.push(endpoint);
            }
            continue;
        }
        if !connect.contains(&endpoint) {
            connect.push(endpoint);
        }
    }

    for ip in swept_ips {
        // A host that told us its port is not worth guessing at.
        if advertised_hosts.contains(&ip.as_str()) {
            continue;
        }
        let endpoint = format!("{ip}:{ADB_NETWORK_PORT}");
        if !connect.contains(&endpoint) {
            connect.push(endpoint);
        }
    }

    // Only report pairing for a device we have no way to reach otherwise;
    // an already-paired TV advertises both services and needs no code.
    let mut needs_pairing: Vec<String> = Vec::new();
    for service in services.iter().filter(|s| s.is_pairing()) {
        if advertised_hosts.contains(&service.host.as_str()) {
            continue;
        }
        let endpoint = service.endpoint();
        if !needs_pairing.contains(&endpoint) {
            needs_pairing.push(endpoint);
        }
    }

    ScanTargets {
        connect,
        already_attached,
        needs_pairing,
    }
}

/// Transports adb currently holds, with their state, minus offline ones. Used
/// to avoid handing adb a second key for a device it is already attached to.
///
/// Offline is left out: it is the stale state a scan should recover from by
/// dialling again. Unauthorized stays in: a second key would be a second
/// unauthorized row that can't be merged (neither exposes `ro.serialno`), and
/// dialling again can't answer the prompt on the TV.
async fn attached_serials(adb: &dyn AdbDriver) -> Vec<(String, DeviceStatus)> {
    match adb.raw(&["devices"]).await {
        Ok(out) => parse_device_list(&out.stdout)
            .into_iter()
            .filter(|entry| entry.status != DeviceStatus::Offline)
            .map(|entry| (entry.serial, entry.status))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Offline transport keys adb holds for the service advertising `target`.
///
/// Redialling an advertised endpoint whose mDNS transport has gone offline
/// gives adb a second key for the same device. The offline one reports no
/// `ro.serialno`, so identity-based collapsing can't merge them and the TV
/// shows up twice. These are the keys to drop before dialling.
fn stale_aliases_for(target: &str, services: &[MdnsService], offline: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for service in services.iter().filter(|s| s.endpoint() == target) {
        let prefix = format!("{}.", service.instance);
        for key in offline {
            if (key == &service.instance || key.starts_with(&prefix)) && !out.contains(key) {
                out.push(key.clone());
            }
        }
    }
    out
}

/// `None` when adb could not be asked: an unread list proves nothing about
/// whether a stale alias is gone.
async fn offline_serials(adb: &dyn AdbDriver) -> Option<Vec<String>> {
    match adb.raw(&["devices"]).await {
        Ok(out) => Some(
            parse_device_list(&out.stdout)
                .into_iter()
                .filter(|entry| entry.status == DeviceStatus::Offline)
                .map(|entry| entry.serial)
                .collect(),
        ),
        Err(_) => None,
    }
}

/// Of the advertised endpoints skipped because adb already holds them, which
/// are ready and which are waiting for the user to authorize this computer.
fn split_already_attached(
    already_attached: &[String],
    services: &[MdnsService],
    attached: &[(String, DeviceStatus)],
) -> (Vec<String>, Vec<String>) {
    let mut ready = Vec::new();
    let mut unauthorized = Vec::new();
    for endpoint in already_attached {
        let status = services
            .iter()
            .filter(|s| &s.endpoint() == endpoint)
            .find_map(|s| {
                let prefix = format!("{}.", s.instance);
                attached
                    .iter()
                    .find(|(key, _)| {
                        key != endpoint && (key == &s.instance || key.starts_with(&prefix))
                    })
                    .map(|(_, status)| *status)
            });
        if status == Some(DeviceStatus::Unauthorized) {
            unauthorized.push(endpoint.clone());
        } else {
            ready.push(endpoint.clone());
        }
    }
    (ready, unauthorized)
}

/// Ask the adb daemon what it has seen advertised over mDNS. Requires
/// platform-tools 30+; older binaries print usage text to stderr, which
/// parses to no services rather than an error.
async fn discover_mdns_services(adb: &dyn AdbDriver) -> Vec<MdnsService> {
    match adb.raw(&["mdns", "services"]).await {
        Ok(out) => parse_mdns_services(&out.stdout),
        Err(_) => Vec::new(),
    }
}

/// `scan_network` — sweep the local /24 for ADB-listening devices and try
/// `adb connect` against each responder. Returns a structured summary so the
/// UI can render counts and any per-IP failures.
#[tauri::command]
pub async fn scan_network(state: State<'_, AppState>) -> Result<ScanResult, String> {
    let Some(prefix) = local_subnet_prefix().await else {
        tracing::info!(subnet = "unknown", found = 0, "scan finished");
        return Ok(ScanResult {
            subnet: None,
            found: vec![],
            connected: vec![],
            unauthorized: vec![],
            failed: vec![],
            needs_pairing: vec![],
            message: "Could not detect default gateway. Set SHIELD_OPTIMIZER_SUBNET=\"a.b.c\" \
                      to override, or use Add by IP."
                .to_string(),
        });
    };
    let subnet_label = format!("{}.{}.{}", prefix[0], prefix[1], prefix[2]);

    let hits = scan_subnet(prefix).await;
    let swept: Vec<String> = hits.iter().map(|h| h.ip.clone()).collect();

    let adb = state.adb_snapshot().await;

    // Warm the adb daemon before connecting. The port sweep just opened and
    // dropped raw TCP sockets against each device's adbd; firing `adb connect`
    // immediately afterward — especially against a cold daemon — tends to get
    // a transient refusal, which is why a manual "Restart ADB" (which starts
    // the daemon) made the same devices connect. Starting the server here, plus
    // a single retry below, makes the scan connect on its own.
    let _ = adb.raw(&["start-server"]).await;

    // The daemon has to be up before it can report what it has browsed --
    // and starting it is also what makes adb auto-connect the mDNS devices it
    // already trusts, so read the attached list only after this point.
    let services = discover_mdns_services(adb.as_ref()).await;
    let attached_with_status = attached_serials(adb.as_ref()).await;
    let attached: Vec<String> = attached_with_status
        .iter()
        .map(|(k, _)| k.clone())
        .collect();
    let targets = merge_scan_targets(&swept, &services, &attached);
    let (held_ready, held_unauthorized) =
        split_already_attached(&targets.already_attached, &services, &attached_with_status);

    let found: Vec<String> = targets
        .connect
        .iter()
        .chain(targets.already_attached.iter())
        .chain(targets.needs_pairing.iter())
        .cloned()
        .collect();

    let mut connected = held_ready;
    let mut unauthorized = held_unauthorized;
    let mut failed = Vec::new();
    let offline = offline_serials(adb.as_ref()).await.unwrap_or_default();
    for target in &targets.connect {
        let stale = stale_aliases_for(target, &services, &offline);
        if !stale.is_empty() {
            for key in &stale {
                let _ = adb.raw(&["disconnect", key]).await;
            }
            // Only redial once the offline key is really gone; otherwise the
            // redial is exactly the duplicate this cleanup exists to prevent.
            let gone = match offline_serials(adb.as_ref()).await {
                Some(still_offline) => !stale.iter().any(|key| still_offline.contains(key)),
                None => false,
            };
            if !gone {
                failed.push(target.clone());
                continue;
            }
        }
        let mut outcome = adb_connect(adb.as_ref(), target).await;
        // Only a hard failure is worth retrying — "unauthorized" means the
        // device is waiting for the user to approve the prompt on-screen.
        if outcome == ConnectOutcome::Failed {
            tokio::time::sleep(Duration::from_millis(400)).await;
            outcome = adb_connect(adb.as_ref(), target).await;
        }
        match outcome {
            ConnectOutcome::Connected => connected.push(target.clone()),
            ConnectOutcome::Unauthorized => unauthorized.push(target.clone()),
            ConnectOutcome::Failed => failed.push(target.clone()),
        }
    }

    let message = summary_message(
        &subnet_label,
        found.len(),
        &connected,
        &unauthorized,
        &targets.needs_pairing,
    );
    tracing::info!(
        subnet = %subnet_label,
        found = found.len(),
        connected = connected.len(),
        unauthorized = unauthorized.len(),
        failed = failed.len(),
        needs_pairing = targets.needs_pairing.len(),
        "scan finished"
    );

    Ok(ScanResult {
        subnet: Some(subnet_label),
        found,
        connected,
        unauthorized,
        failed,
        needs_pairing: targets.needs_pairing,
        message,
    })
}

fn summary_message(
    subnet_label: &str,
    found: usize,
    connected: &[String],
    unauthorized: &[String],
    needs_pairing: &[String],
) -> String {
    if found == 0 {
        return format!(
            "No devices on {subnet_label}.x answered on the ADB port. Make sure Network \
             debugging or Wireless debugging is on. A device that only offers Wireless \
             debugging may need Pair PIN first."
        );
    }
    let mut message = format!(
        "Scanned {subnet_label}.x — found {found} device{}, connected {}.",
        if found == 1 { "" } else { "s" },
        connected.len()
    );
    if !unauthorized.is_empty() {
        message.push_str(&format!(
            " {} need{} authorization — accept the debugging prompt on the TV, then \
             Refresh.",
            unauthorized.len(),
            if unauthorized.len() == 1 { "s" } else { "" }
        ));
    }
    if !needs_pairing.is_empty() {
        message.push_str(&format!(
            " {} waiting to be paired — on the TV open Wireless debugging → Pair device \
             with pairing code, then use Pair PIN with {}.",
            needs_pairing.len(),
            needs_pairing.join(", ")
        ));
    }
    message
}

/// `local_address_for` — the local IPv4 address this computer would use to
/// reach `host`, for the Pair PIN "typo?" hint. Asks the OS routing table by
/// connecting an unbound UDP socket, which sends nothing. `None` for anything
/// that is not an IPv4 literal, or when there is no route.
#[tauri::command]
pub async fn local_address_for(host: String) -> Result<Option<String>, String> {
    let Ok(ip) = host.trim().parse::<std::net::Ipv4Addr>() else {
        return Ok(None);
    };
    let local = std::net::UdpSocket::bind(("0.0.0.0", 0))
        .and_then(|socket| {
            socket.connect((ip, 9))?;
            socket.local_addr()
        })
        .ok()
        .map(|addr| addr.ip())
        .filter(|ip| !ip.is_unspecified() && !ip.is_loopback());
    Ok(local.map(|ip| ip.to_string()))
}

// Live, against a real Wireless debugging device that is already paired and
// attached. Ignored in CI; run by hand:
//   SHIELD_TEST_PAIR_ADDRESS=192.168.42.211:45439 SHIELD_TEST_HARDWARE_ID=58040DLCH005YV \
//   SHIELD_TEST_MDNS_INSTANCE=adb-58040DLCH005YV-jBeCEe \
//     cargo test -p shield-optimizer-v2 live_forget_then_auto_connect -- --ignored --nocapture
// Forgets every transport of that hardware id, checks none is left, then does
// what the Pair PIN flow does after a pair: polls mDNS for the connect port
// and dials it. Disconnect and reconnect only; nothing on the device changes.
#[cfg(test)]
mod live {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use shield_optimizer_core::commands::devices::{
        connect_paired_impl, forget_device_impl, list_devices_impl, probe_paired_connect_impl,
        PairedConnectProbe,
    };
    use shield_optimizer_core::engine::AppListBundle;

    use super::AppState;
    use crate::adb::{parse_device_list, AdbDriver, SubprocessAdb};

    async fn serials_with_id(adb: &dyn AdbDriver, id: &str) -> Vec<String> {
        let out = adb.raw(&["devices"]).await.expect("adb devices");
        let mut hits = Vec::new();
        for entry in parse_device_list(&out.stdout) {
            if let Ok(p) = adb.shell(&entry.serial, "getprop ro.serialno").await {
                if p.stdout.trim() == id {
                    hits.push(entry.serial);
                }
            }
        }
        hits
    }

    #[tokio::test]
    #[ignore]
    async fn live_forget_then_auto_connect() {
        let (Ok(pair_address), Ok(id), Ok(instance)) = (
            std::env::var("SHIELD_TEST_PAIR_ADDRESS"),
            std::env::var("SHIELD_TEST_HARDWARE_ID"),
            std::env::var("SHIELD_TEST_MDNS_INSTANCE"),
        ) else {
            eprintln!(
                "set SHIELD_TEST_PAIR_ADDRESS, SHIELD_TEST_HARDWARE_ID and \
                 SHIELD_TEST_MDNS_INSTANCE to run"
            );
            return;
        };
        let adb: Arc<dyn AdbDriver> = Arc::new(SubprocessAdb::discover().expect("adb binary"));
        let state = AppState::new(adb.clone(), AppListBundle::default(), std::env::temp_dir());

        let before = serials_with_id(adb.as_ref(), &id).await;
        eprintln!("transports before: {before:?}");
        let first = before
            .first()
            .expect("the device must be attached to start");

        let forgot = forget_device_impl(&state, first).await.expect("forget");
        eprintln!("forget: {forgot:?}");
        assert!(forgot.ok);
        let after = serials_with_id(adb.as_ref(), &id).await;
        eprintln!("transports right after forget: {after:?}");
        assert!(
            after.is_empty(),
            "every alias must be gone on the first click"
        );

        let started = Instant::now();
        let serial = loop {
            let probe = probe_paired_connect_impl(&state, &pair_address, Some(&instance))
                .await
                .expect("probe");
            eprintln!("{:>5.1}s probe: {probe:?}", started.elapsed().as_secs_f32());
            match probe {
                PairedConnectProbe::Attached { serial } => break serial,
                PairedConnectProbe::Endpoint { address } => {
                    let r = connect_paired_impl(&state, &address, &instance)
                        .await
                        .expect("connect");
                    eprintln!("connect_paired {address}: {r:?}");
                    assert!(!r.not_the_paired_device, "{}", r.message);
                    if r.ok {
                        break address;
                    }
                }
                PairedConnectProbe::Ambiguous { addresses } => panic!("ambiguous: {addresses:?}"),
                PairedConnectProbe::Unidentified => panic!("no instance"),
                PairedConnectProbe::NotThePairedDevice { message } => panic!("{message}"),
                PairedConnectProbe::Waiting => {}
            }
            assert!(started.elapsed() < Duration::from_secs(45), "timed out");
            tokio::time::sleep(Duration::from_millis(1500)).await;
        };

        let devices = list_devices_impl(&state).await.expect("list");
        let row = devices
            .iter()
            .find(|d| d.serial == serial)
            .expect("the connected transport is a listed row");
        eprintln!("row: {} {} ({:?})", row.serial, row.name, row.status);
        assert_eq!(
            row.properties.as_ref().map(|p| p.serial_number.as_str()),
            Some(id.as_str())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn service(instance: &str, kind: &str, host: &str, port: u16) -> MdnsService {
        MdnsService {
            instance: instance.into(),
            service: kind.into(),
            host: host.into(),
            port,
        }
    }

    #[test]
    fn a_redial_drops_the_offline_alias_of_the_same_service_only() {
        let services = vec![
            service(
                "adb-SERIALA-abc",
                "_adb-tls-connect._tcp",
                "192.168.1.9",
                41541,
            ),
            service(
                "adb-SERIALB-def",
                "_adb-tls-connect._tcp",
                "192.168.1.10",
                40000,
            ),
        ];
        let offline = vec![
            "adb-SERIALA-abc._adb-tls-connect._tcp".to_string(),
            "adb-SERIALB-def._adb-tls-connect._tcp".to_string(),
            "192.168.1.20:5555".to_string(),
        ];
        assert_eq!(
            stale_aliases_for("192.168.1.9:41541", &services, &offline),
            vec!["adb-SERIALA-abc._adb-tls-connect._tcp".to_string()]
        );
        assert!(stale_aliases_for("192.168.1.30:5555", &services, &offline).is_empty());
    }

    #[test]
    fn an_advertised_port_is_used_instead_of_guessing_5555() {
        // The #88 device: Android 14, wireless debugging on a random port.
        // A :5555 sweep never sees it, and connecting to :5555 never works.
        let services = vec![service(
            "adb-58040DLCH005YV-jBeCEe",
            crate::adb::MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            41541,
        )];

        let targets = merge_scan_targets(&[], &services, &[]);

        assert_eq!(targets.connect, vec!["192.168.42.211:41541"]);
        assert!(targets.needs_pairing.is_empty());
    }

    #[test]
    fn an_advertised_host_is_not_also_probed_on_5555() {
        // The sweep can still see the host (some devices listen on both), but
        // the advertised port is the one that is actually current.
        let services = vec![service(
            "adb-tv",
            crate::adb::MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            41541,
        )];
        let swept = vec!["192.168.42.211".to_string(), "192.168.42.71".to_string()];

        let targets = merge_scan_targets(&swept, &services, &[]);

        assert_eq!(
            targets.connect,
            vec!["192.168.42.211:41541", "192.168.42.71:5555"]
        );
        assert!(!targets.connect.iter().any(|t| t == "192.168.42.211:5555"));
    }

    #[test]
    fn legacy_devices_still_reach_5555_when_nothing_is_advertised() {
        // The Shield path, unchanged: no mDNS, swept on the standard port.
        let targets = merge_scan_targets(&["192.168.42.71".to_string()], &[], &[]);

        assert_eq!(targets.connect, vec!["192.168.42.71:5555"]);
        assert!(targets.needs_pairing.is_empty());
    }

    #[test]
    fn a_pairing_only_device_is_reported_rather_than_connected_to() {
        // Connecting to the pairing port always fails, and reporting it as a
        // failure tells the user nothing. It needs a code from the TV.
        let services = vec![service(
            "adb-tcl",
            crate::adb::MDNS_SERVICE_PAIRING,
            "192.168.42.211",
            37199,
        )];

        let targets = merge_scan_targets(&[], &services, &[]);

        assert!(targets.connect.is_empty());
        assert_eq!(targets.needs_pairing, vec!["192.168.42.211:37199"]);
    }

    #[test]
    fn an_already_paired_device_is_not_asked_to_pair_again() {
        // A paired TV advertises both services; only the connect one matters.
        let services = vec![
            service(
                "adb-tcl-pair",
                crate::adb::MDNS_SERVICE_PAIRING,
                "192.168.42.211",
                37199,
            ),
            service(
                "adb-tcl-connect",
                crate::adb::MDNS_SERVICE_CONNECT,
                "192.168.42.211",
                41541,
            ),
        ];

        let targets = merge_scan_targets(&[], &services, &[]);

        assert_eq!(targets.connect, vec!["192.168.42.211:41541"]);
        assert!(targets.needs_pairing.is_empty());
    }

    #[test]
    fn repeated_advertisements_produce_one_target_each() {
        let services = vec![
            service("a", crate::adb::MDNS_SERVICE_LEGACY, "192.168.42.71", 5555),
            service("a", crate::adb::MDNS_SERVICE_LEGACY, "192.168.42.71", 5555),
        ];

        let targets = merge_scan_targets(&["192.168.42.71".to_string()], &services, &[]);

        assert_eq!(targets.connect, vec!["192.168.42.71:5555"]);
    }

    /// The regression this whole layer exists for. adb auto-connects a paired
    /// mDNS device under its service name; connecting the same device again by
    /// host:port makes adb register a second transport, and the device list
    /// faithfully renders both.
    #[test]
    fn a_device_adb_already_holds_is_not_connected_a_second_time() {
        let services = vec![service(
            "adb-58040DLCH005YV-jBeCEe",
            crate::adb::MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            34083,
        )];
        let attached = vec!["adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp".to_string()];

        let targets = merge_scan_targets(&[], &services, &attached);

        assert!(
            targets.connect.is_empty(),
            "reconnecting by host:port is what creates the duplicate: {:?}",
            targets.connect
        );
        assert!(targets.needs_pairing.is_empty());
        // Codex on #128: not dialling it must not also drop it from the
        // scan's found/connected counts.
        assert_eq!(targets.already_attached, vec!["192.168.42.211:34083"]);
    }

    #[tokio::test]
    async fn an_offline_transport_does_not_stop_the_scan_redialling() {
        // Codex on #128: a stale offline mDNS transport used to count as
        // attached, so the advertised endpoint was never dialled again.
        use shield_optimizer_core::commands::test_support::MockAdb;
        let mock = MockAdb::default().on_raw(
            "devices",
            "List of devices attached\n\
             adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp\toffline\n\
             192.168.42.71:5555\tdevice\n",
        );
        let attached = attached_serials(&mock).await;
        assert_eq!(
            attached,
            vec![("192.168.42.71:5555".to_string(), DeviceStatus::Device)]
        );
        let keys: Vec<String> = attached.iter().map(|(k, _)| k.clone()).collect();

        let services = vec![service(
            "adb-58040DLCH005YV-jBeCEe",
            crate::adb::MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            34083,
        )];
        let targets = merge_scan_targets(&[], &services, &keys);
        assert_eq!(targets.connect, vec!["192.168.42.211:34083"]);
        assert!(targets.already_attached.is_empty());
    }

    #[tokio::test]
    async fn an_unauthorized_transport_is_not_dialled_again_and_reports_unauthorized() {
        // Codex on #128: redialling an unauthorized mDNS key by host:port
        // makes a second unauthorized row that can never be merged, and
        // cannot answer the prompt on the TV anyway.
        use shield_optimizer_core::commands::test_support::MockAdb;
        let mock = MockAdb::default().on_raw(
            "devices",
            "List of devices attached\n\
             adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp\tunauthorized\n\
             adb-1324619053514-aB1._adb-tls-connect._tcp\tdevice\n",
        );
        let attached = attached_serials(&mock).await;
        let keys: Vec<String> = attached.iter().map(|(k, _)| k.clone()).collect();
        let services = vec![
            service(
                "adb-58040DLCH005YV-jBeCEe",
                crate::adb::MDNS_SERVICE_CONNECT,
                "192.168.42.211",
                34083,
            ),
            service(
                "adb-1324619053514-aB1",
                crate::adb::MDNS_SERVICE_CONNECT,
                "192.168.42.196",
                40001,
            ),
        ];
        let targets = merge_scan_targets(&[], &services, &keys);
        assert!(targets.connect.is_empty(), "{:?}", targets.connect);

        let (ready, unauthorized) =
            split_already_attached(&targets.already_attached, &services, &attached);
        assert_eq!(ready, vec!["192.168.42.196:40001"]);
        assert_eq!(unauthorized, vec!["192.168.42.211:34083"]);
    }

    #[test]
    fn an_advertised_host_adb_already_holds_is_still_not_probed_on_5555() {
        // Skipping the connect must not also forfeit the knowledge that this
        // host told us its real port -- otherwise the sweep guesses :5555 for
        // a device we already know is somewhere else.
        let services = vec![service(
            "adb-tv",
            crate::adb::MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            34083,
        )];
        let attached = vec!["adb-tv._adb-tls-connect._tcp".to_string()];

        let targets = merge_scan_targets(&["192.168.42.211".to_string()], &services, &attached);

        assert!(targets.connect.is_empty(), "{:?}", targets.connect);
    }

    #[test]
    fn a_device_attached_at_the_very_endpoint_we_would_dial_is_still_dialled() {
        // Redialling the same key cannot duplicate anything — adb just says
        // "already connected". Skipping it would drop the device from the
        // scan's connected count and read as "your TV wasn't found".
        let services = vec![service(
            "adb-tv",
            crate::adb::MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            34083,
        )];
        let attached = vec!["192.168.42.211:34083".to_string()];

        let targets = merge_scan_targets(&[], &services, &attached);

        assert_eq!(targets.connect, vec!["192.168.42.211:34083"]);
    }

    #[test]
    fn a_device_adb_does_not_hold_is_still_connected() {
        // The #88 fix has to survive this layer: an advertised device adb has
        // never seen must still be dialled at its real port.
        let services = vec![service(
            "adb-new-tv",
            crate::adb::MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            34083,
        )];
        let attached = vec!["adb-some-other-device._adb-tls-connect._tcp".to_string()];

        let targets = merge_scan_targets(&[], &services, &attached);

        assert_eq!(targets.connect, vec!["192.168.42.211:34083"]);
    }

    #[test]
    fn a_similar_instance_name_is_not_treated_as_the_same_device() {
        // Prefix matching must stop at the dot. `adb-tv-2` is not `adb-tv`.
        let services = vec![service(
            "adb-tv",
            crate::adb::MDNS_SERVICE_CONNECT,
            "192.168.42.211",
            34083,
        )];
        let attached = vec!["adb-tv-2._adb-tls-connect._tcp".to_string()];

        let targets = merge_scan_targets(&[], &services, &attached);

        assert_eq!(targets.connect, vec!["192.168.42.211:34083"]);
    }

    #[test]
    fn legacy_sweep_targets_are_unaffected_by_the_attached_list() {
        // A Shield on Network debugging advertises nothing and is swept. Being
        // attached under its own ip:port key is the normal steady state and
        // must not stop the scan reporting it.
        let targets = merge_scan_targets(
            &["192.168.42.196".to_string()],
            &[],
            &["192.168.42.196:5555".to_string()],
        );

        assert_eq!(targets.connect, vec!["192.168.42.196:5555"]);
    }

    /// End-to-end over output captured verbatim from a real LAN while the
    /// duplicate was reproducing: a phone adb had auto-connected over mDNS,
    /// a Shield on legacy network debugging, and three unauthorized hosts.
    #[test]
    fn real_world_scan_output_connects_the_shield_and_leaves_the_phone_alone() {
        let services = parse_mdns_services(
            "List of discovered mdns services\n\
             adb-58040DLCH005YV-jBeCEe\t_adb-tls-connect._tcp\t192.168.42.211:34083\n\
             adb-1324619053514\t_adb._tcp\t192.168.42.196:5555\n\
             adb-1321920044953\t_adb._tcp\t192.168.42.143:5555\n",
        );
        let attached: Vec<String> = parse_device_list(
            "List of devices attached\n\
             192.168.42.143:5555\tunauthorized\n\
             192.168.42.196:5555\tdevice\n\
             adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp\tdevice\n",
        )
        .into_iter()
        .map(|e| e.serial)
        .collect();
        let swept = vec![
            "192.168.42.143".to_string(),
            "192.168.42.196".to_string(),
            "192.168.42.25".to_string(),
        ];

        let targets = merge_scan_targets(&swept, &services, &attached);

        assert!(
            !targets
                .connect
                .iter()
                .any(|t| t.starts_with("192.168.42.211")),
            "the phone is already attached under its mDNS name: {:?}",
            targets.connect
        );
        // Everything else still gets dialled, including a host adb already
        // holds under the same ip:port key -- reconnecting that is a no-op for
        // adb and keeps the scan's "connected" count honest.
        assert!(targets.connect.contains(&"192.168.42.196:5555".to_string()));
        assert!(targets.connect.contains(&"192.168.42.143:5555".to_string()));
        assert!(targets.connect.contains(&"192.168.42.25:5555".to_string()));
        assert!(targets.needs_pairing.is_empty());
    }

    #[test]
    fn summary_tells_the_user_how_to_pair_a_waiting_device() {
        let message = summary_message(
            "192.168.42",
            1,
            &[],
            &[],
            &["192.168.42.211:37199".to_string()],
        );

        assert!(message.contains("1 waiting to be paired"));
        assert!(message.contains("192.168.42.211:37199"));
        assert!(message.contains("Pair PIN"));
    }

    #[test]
    fn summary_says_nothing_about_pairing_when_nothing_is_waiting() {
        let message = summary_message(
            "192.168.42",
            1,
            &["192.168.42.71:5555".to_string()],
            &[],
            &[],
        );

        assert!(!message.contains("paired"));
    }

    #[test]
    fn classifies_fresh_and_already_connected() {
        assert_eq!(
            classify_connect_output("connected to 192.168.42.71:5555"),
            ConnectOutcome::Connected
        );
        assert_eq!(
            classify_connect_output("already connected to 192.168.42.71:5555"),
            ConnectOutcome::Connected
        );
    }

    #[test]
    fn classifies_connected_despite_daemon_startup_noise() {
        let out = "* daemon not running; starting now at tcp:5037\n\
                   * daemon started successfully\nconnected to 192.168.42.71:5555";
        assert_eq!(classify_connect_output(out), ConnectOutcome::Connected);
    }

    #[test]
    fn classifies_unauthorized() {
        // Real output from platform-tools 37.0.0 against a TV that hasn't
        // approved this host's key — exits 0, device lands in `adb devices`
        // as `unauthorized`.
        assert_eq!(
            classify_connect_output("failed to authenticate to 192.168.42.143:5555"),
            ConnectOutcome::Unauthorized
        );
    }

    #[test]
    fn classifies_failures() {
        assert_eq!(
            classify_connect_output("failed to connect to '192.168.42.9:5555': Connection refused"),
            ConnectOutcome::Failed
        );
        assert_eq!(
            classify_connect_output("cannot connect to 192.168.42.9:5555: timeout"),
            ConnectOutcome::Failed
        );
        assert_eq!(classify_connect_output(""), ConnectOutcome::Failed);
    }

    #[test]
    fn summary_mentions_unauthorized_devices() {
        let msg = summary_message(
            "192.168.42",
            4,
            &[],
            &["192.168.42.143".into(), "192.168.42.25".into()],
            &[],
        );
        assert_eq!(
            msg,
            "Scanned 192.168.42.x — found 4 devices, connected 0. 2 need authorization — \
             accept the debugging prompt on the TV, then Refresh."
        );
    }

    #[test]
    fn summary_plain_when_all_connected() {
        let msg = summary_message("10.0.0", 1, &["10.0.0.5".into()], &[], &[]);
        assert_eq!(msg, "Scanned 10.0.0.x — found 1 device, connected 1.");
    }
}
