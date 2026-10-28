//! Observation-only device discovery, independent of the selected ADB transport.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::adb::discovery::{DiscoveryManager, DiscoveryProvider};
use shield_optimizer_core::adb::{AdbError, NativeDiscoveryReport};

use super::AppState;

/// The passive browser owns no ADB identity, device session or host server.
pub struct PassiveDiscoveryState {
    manager: Arc<DiscoveryManager>,
}

impl PassiveDiscoveryState {
    fn with_provider(provider: Arc<dyn DiscoveryProvider>) -> Self {
        Self {
            manager: DiscoveryManager::new(provider),
        }
    }

    #[cfg(feature = "e2e")]
    pub fn fixture(mdns: impl Fn() -> String + Send + Sync + 'static) -> Self {
        Self::with_provider(Arc::new(FixtureDiscoveryProvider(Box::new(mdns))))
    }

    async fn snapshot(&self, state: &AppState) -> Result<NativeDiscoveryReport, AdbError> {
        let lease = Arc::new(state.try_adb_operation()?);
        if self.manager.cleanup_pending() {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                self.manager.retry_cleanup(),
            )
            .await;
        }
        self.manager.snapshot(lease).await
    }
}

impl Drop for PassiveDiscoveryState {
    fn drop(&mut self) {
        self.manager.cancel();
    }
}

impl Default for PassiveDiscoveryState {
    fn default() -> Self {
        #[cfg(not(feature = "e2e"))]
        {
            Self::with_provider(Arc::new(crate::adb::discovery::MdnsProvider::default()))
        }
        #[cfg(feature = "e2e")]
        {
            Self::fixture(String::new)
        }
    }
}

#[cfg(feature = "e2e")]
struct FixtureDiscoveryProvider(Box<dyn Fn() -> String + Send + Sync>);

#[cfg(feature = "e2e")]
impl DiscoveryProvider for FixtureDiscoveryProvider {
    fn collect(&self, _: &std::sync::atomic::AtomicBool) -> crate::adb::discovery::DiscoveryBatch {
        use crate::adb::discovery::{DiscoveryBatch, DiscoveryEvent};
        use shield_optimizer_core::adb::{
            parse_mdns_services, NativeAdvertisement, NativeServiceKind, MDNS_SERVICE_CONNECT,
            MDNS_SERVICE_LEGACY, MDNS_SERVICE_PAIRING,
        };
        let observed = std::time::Instant::now();
        let events = parse_mdns_services(&(self.0)())
            .into_iter()
            .filter_map(|service| {
                let kind = match service.service.as_str() {
                    MDNS_SERVICE_LEGACY => NativeServiceKind::LegacyAdb,
                    MDNS_SERVICE_CONNECT => NativeServiceKind::TlsConnect,
                    MDNS_SERVICE_PAIRING => NativeServiceKind::TlsPairing,
                    _ => return None,
                };
                let instance = format!("{}.{}.local.", service.instance, service.service);
                let row = NativeAdvertisement {
                    service_kind: kind,
                    instance: instance.clone(),
                    address: service.host.clone(),
                    port: service.port,
                    interface_name: Some("fixture".into()),
                    interface_index: Some(1),
                    scope_id: None,
                    connect_address: Some(service.endpoint()),
                    observed_age_ms: 0,
                    cached: false,
                    warning: None,
                };
                Some(DiscoveryEvent::Resolved {
                    kind,
                    instance,
                    rows: vec![row],
                    observed,
                })
            })
            .collect();
        DiscoveryBatch {
            events,
            warnings: Vec::new(),
            cleanup_confirmed: true,
        }
    }
    fn retry_cleanup(&self) -> bool {
        true
    }
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_discovery: Option<shield_optimizer_core::adb::NativeDiscoveryReport>,
}

/// Observe advertisements only. Connecting and pairing are separate explicit actions.
#[tauri::command]
pub async fn scan_network(
    state: State<'_, AppState>,
    discovery: State<'_, PassiveDiscoveryState>,
) -> Result<ScanResult, String> {
    scan_network_impl(state.inner(), discovery.inner()).await
}

async fn scan_network_impl(
    state: &AppState,
    discovery: &PassiveDiscoveryState,
) -> Result<ScanResult, String> {
    let selected = state.adb_snapshot().await;
    let report = if selected.uses_host_server() {
        discovery.snapshot(state).await
    } else {
        selected.discover_network().await
    };
    let native_discovery = report.map_err(|error| match error {
        AdbError::Initialization(_) => error.to_string(),
        _ => format!("Discovery: {error}. Use Add by IP or Pair PIN."),
    })?;
    Ok(ScanResult {
        subnet: None,
        found: Vec::new(),
        connected: Vec::new(),
        unauthorized: Vec::new(),
        failed: Vec::new(),
        needs_pairing: Vec::new(),
        message: "Discovery complete. No connections changed.".into(),
        native_discovery: Some(native_discovery),
    })
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
    use crate::adb::discovery::{DiscoveryBatch, DiscoveryEvent};
    use crate::adb::AdbDriver;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct PassiveFixture {
        calls: Arc<AtomicUsize>,
        unavailable: bool,
        cleanup: Arc<AtomicBool>,
    }

    impl DiscoveryProvider for PassiveFixture {
        fn collect(&self, _: &AtomicBool) -> DiscoveryBatch {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if self.unavailable {
                return DiscoveryBatch {
                    events: Vec::new(),
                    warnings: vec![
                        "Simulated multicast unavailable; use Add by IP or Pair PIN.".into(),
                    ],
                    cleanup_confirmed: true,
                };
            }
            let fixture: NativeDiscoveryReport = serde_json::from_str(include_str!(
                "../../../tests/fixtures/native-discovery.json"
            ))
            .unwrap();
            let events = fixture
                .advertisements
                .into_iter()
                .take(4)
                .map(|row| DiscoveryEvent::Resolved {
                    kind: row.service_kind,
                    instance: format!("{}-{}-{}", row.instance, row.address, row.port),
                    observed: std::time::Instant::now(),
                    rows: vec![row],
                })
                .collect();
            DiscoveryBatch {
                events,
                warnings: Vec::new(),
                cleanup_confirmed: self.cleanup.load(Ordering::Relaxed),
            }
        }
        fn retry_cleanup(&self) -> bool {
            self.cleanup.load(Ordering::Relaxed)
        }
    }

    fn fixture_discovery(unavailable: bool) -> PassiveDiscoveryState {
        PassiveDiscoveryState::with_provider(Arc::new(PassiveFixture {
            calls: Arc::new(AtomicUsize::new(0)),
            unavailable,
            cleanup: Arc::new(AtomicBool::new(true)),
        }))
    }

    struct SubprocessSelectionMock;
    #[async_trait::async_trait]
    impl AdbDriver for SubprocessSelectionMock {
        async fn raw(&self, _: &[&str]) -> crate::adb::AdbResult<crate::adb::AdbOutput> {
            panic!("passive scan must not start, probe, connect, pair, disconnect or authenticate through ADB")
        }
        async fn shell(&self, _: &str, _: &str) -> crate::adb::AdbResult<crate::adb::AdbOutput> {
            panic!("passive scan must not query a device")
        }
        async fn discover_network(&self) -> crate::adb::AdbResult<NativeDiscoveryReport> {
            panic!(
                "subprocess scan must use the standalone observer, not change or invoke the driver"
            )
        }
    }

    fn subprocess_state() -> AppState {
        AppState::new(
            Arc::new(SubprocessSelectionMock),
            crate::engine::AppListBundle::default(),
            std::path::PathBuf::from("unused-passive-discovery-no-identity"),
        )
    }

    #[tokio::test]
    async fn subprocess_scan_observes_all_service_types_without_any_adb_or_device_call() {
        let state = subprocess_state();
        let discovery = fixture_discovery(false);
        let result = scan_network_impl(&state, &discovery).await.unwrap();
        assert!(result.subnet.is_none());
        assert!(result.found.is_empty() && result.connected.is_empty());
        assert!(
            result.failed.is_empty()
                && result.unauthorized.is_empty()
                && result.needs_pairing.is_empty()
        );
        let report = result.native_discovery.unwrap();
        assert_eq!(report.advertisements.len(), 4);
        assert!(report.advertisements.iter().all(|row| !row.cached));
        assert!(report.advertisements.iter().any(
            |row| row.service_kind == shield_optimizer_core::adb::NativeServiceKind::TlsPairing
        ));
        assert!(report
            .advertisements
            .iter()
            .any(|row| row.connect_address.as_deref() == Some("[fe80::1234%7]:37333")));
        assert!(
            state.adb_snapshot().await.uses_host_server(),
            "the selected transport was not changed"
        );
        assert_eq!(state.adb_operations_active(), 0);
    }

    #[tokio::test]
    async fn unavailable_passive_discovery_keeps_manual_fallback_without_adb_fanout() {
        let state = subprocess_state();
        let result = scan_network_impl(&state, &fixture_discovery(true))
            .await
            .unwrap();
        let report = result.native_discovery.unwrap();
        assert!(report.advertisements.is_empty());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("Add by IP or Pair PIN")));
        assert_eq!(state.adb_operations_active(), 0);
    }

    #[tokio::test]
    async fn unresolved_browser_cleanup_blocks_another_browse_and_stays_counted() {
        let state = subprocess_state();
        let calls = Arc::new(AtomicUsize::new(0));
        let cleanup = Arc::new(AtomicBool::new(false));
        let discovery = PassiveDiscoveryState::with_provider(Arc::new(PassiveFixture {
            calls: calls.clone(),
            unavailable: false,
            cleanup: cleanup.clone(),
        }));
        scan_network_impl(&state, &discovery).await.unwrap();
        assert!(state.adb_operations_active() > 0);
        assert!(scan_network_impl(&state, &discovery)
            .await
            .err()
            .unwrap()
            .contains("cleanup"));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        cleanup.store(true, Ordering::Relaxed);
        scan_network_impl(&state, &discovery).await.unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert_eq!(state.adb_operations_active(), 0);
    }

    #[tokio::test]
    async fn quit_admission_refuses_a_scan_before_any_browser_starts() {
        let state = subprocess_state();
        let calls = Arc::new(AtomicUsize::new(0));
        let discovery = PassiveDiscoveryState::with_provider(Arc::new(PassiveFixture {
            calls: calls.clone(),
            unavailable: false,
            cleanup: Arc::new(AtomicBool::new(true)),
        }));
        state.close_adb_admission();
        assert!(scan_network_impl(&state, &discovery).await.is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    struct NativeObservationMock {
        fail: bool,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AdbDriver for NativeObservationMock {
        fn uses_host_server(&self) -> bool {
            false
        }
        async fn raw(&self, _: &[&str]) -> crate::adb::AdbResult<crate::adb::AdbOutput> {
            panic!("native discovery must not use host commands or dial endpoints")
        }
        async fn shell(&self, _: &str, _: &str) -> crate::adb::AdbResult<crate::adb::AdbOutput> {
            panic!("native discovery must not query a TV")
        }
        async fn discover_network(
            &self,
        ) -> crate::adb::AdbResult<shield_optimizer_core::adb::NativeDiscoveryReport> {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if self.fail {
                return Err(crate::adb::AdbError::Transport(
                    "simulated multicast unavailable".into(),
                ));
            }
            Ok(serde_json::from_str(include_str!(
                "../../../tests/fixtures/native-discovery.json"
            ))
            .unwrap())
        }
    }

    fn native_observation_state(
        fail: bool,
    ) -> (AppState, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let state = AppState::new(
            std::sync::Arc::new(NativeObservationMock {
                fail,
                calls: calls.clone(),
            }),
            crate::engine::AppListBundle::default(),
            std::path::PathBuf::from("unused-native-discovery-fixture"),
        );
        (state, calls)
    }

    #[tokio::test]
    async fn native_scan_returns_typed_observations_without_legacy_actions() {
        let (state, calls) = native_observation_state(false);
        let result = scan_network_impl(&state, &fixture_discovery(false))
            .await
            .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(result.subnet.is_none());
        assert!(result.connected.is_empty());
        assert!(result.found.is_empty());
        assert!(result.unauthorized.is_empty());
        assert!(result.failed.is_empty());
        assert!(result.needs_pairing.is_empty());
        let serialized = serde_json::to_value(&result).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/native-discovery.json"
        ))
        .unwrap();
        assert_eq!(serialized["native_discovery"], fixture);
        let ads = &result.native_discovery.unwrap().advertisements;
        assert_eq!(
            ads.len(),
            7,
            "same address/instance does not merge physical identities"
        );
        assert_eq!(ads[1].port, 37111);
        assert_eq!(ads[2].port, 37222);
        assert_eq!(
            ads[3].connect_address.as_deref(),
            Some("[fe80::1234%7]:37333")
        );
        if let Ok(directory) = std::env::var("NATIVE_DISCOVERY_RESULT_EVIDENCE") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join("rust-native-scan-result.json"),
                serde_json::to_vec_pretty(&serialized).unwrap(),
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn native_scan_failure_preserves_manual_fallback_without_host_commands() {
        let (state, calls) = native_observation_state(true);
        let error = scan_network_impl(&state, &fixture_discovery(false))
            .await
            .err()
            .unwrap();
        assert!(error.contains("simulated multicast unavailable"));
        assert!(error.contains("Add by IP or Pair PIN"));
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
    #[tokio::test]
    async fn blocked_initialization_is_not_reported_as_unsupported_discovery() {
        let state = AppState::new(
            std::sync::Arc::new(crate::ConfigurationError(
                "Rust ADB setup failed; existing identity preserved.".into(),
            )),
            crate::engine::AppListBundle::default(),
            std::path::PathBuf::from("unused-initialization-fixture"),
        );
        let message = scan_network_impl(&state, &fixture_discovery(false))
            .await
            .err()
            .expect("initialization must fail");
        assert!(message.contains("Rust ADB setup failed"));
        assert!(!message.contains("unsupported"));
        assert!(!message.contains("Use Add by IP"));
        assert!(!message.contains("Pair PIN"));
    }
}
