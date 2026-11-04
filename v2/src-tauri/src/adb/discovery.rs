//! Observation-only mDNS snapshots. No endpoint is dialed by this module.
use atv_optimizer_core::adb::{AdbError, AdbResult, Lease};
use mdns_sd::{DaemonEvent, DaemonStatus, ScopedIp, ServiceDaemon, ServiceEvent};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    mem::ManuallyDrop,
    net::IpAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const WINDOW: Duration = Duration::from_secs(6);
const MAX_AGE: Duration = Duration::from_secs(30);
const MAX_ROWS: usize = 256;
const MAX_EVENTS: usize = 1024;
const SERVICES: [(&str, TvSource); 2] = [
    ("_googlecast._tcp.local.", TvSource::GoogleCast),
    ("_androidtvremote2._tcp.local.", TvSource::AndroidTvRemote),
];

/// Unauthenticated display observations. No ADB identity or service port crosses this DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TvSource {
    GoogleCast,
    AndroidTvRemote,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TvObservation {
    pub source: TvSource,
    pub instance: String,
    pub friendly_name: Option<String>,
    pub model: Option<String>,
    pub address: String,
    pub interface_name: Option<String>,
    pub interface_index: Option<u32>,
    pub scope_id: Option<u32>,
    /// A usable host for manual setup, with no port. Never an ADB endpoint.
    pub setup_host: Option<String>,
    pub observed_age_ms: u64,
    pub cached: bool,
    pub warning: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TvDiscoveryReport {
    pub observations: Vec<TvObservation>,
    pub warnings: Vec<String>,
    pub observation_max_age_ms: u64,
}

type Key = (TvSource, String);
pub(crate) enum DiscoveryEvent {
    Resolved {
        kind: TvSource,
        instance: String,
        rows: Vec<TvObservation>,
        observed: Instant,
    },
    Removed {
        kind: TvSource,
        instance: String,
    },
}
pub(crate) struct DiscoveryBatch {
    pub events: Vec<DiscoveryEvent>,
    pub warnings: Vec<String>,
    pub cleanup_confirmed: bool,
}
pub(crate) trait DiscoveryProvider: Send + Sync {
    fn collect(&self, cancel: &AtomicBool) -> DiscoveryBatch;
    fn retry_cleanup(&self) -> bool;
}

#[derive(Default)]
pub(crate) struct MdnsProvider {
    pending: Mutex<Option<ServiceDaemon>>,
}
fn kind(service: &str) -> Option<TvSource> {
    SERVICES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(service))
        .map(|(_, kind)| *kind)
}
fn interface_name(name: &str) -> Option<String> {
    (!name.is_empty()).then(|| name.chars().filter(|c| !c.is_control()).take(128).collect())
}
fn row(
    kind: TvSource,
    instance: &str,
    ip: IpAddr,
    name: Option<String>,
    index: Option<u32>,
    ambiguous: bool,
) -> TvObservation {
    let scope = if ip.is_ipv6() { index } else { None };
    let invalid = ip.is_unspecified() || ip.is_multicast() || ip.is_loopback();
    let (endpoint, warning) = if invalid {
        (
            None,
            Some("This advertisement is not a usable remote network address.".into()),
        )
    } else {
        match ip {
            IpAddr::V4(ip) if ip.is_link_local() || ambiguous => (
                None,
                Some(
                    "This address needs an explicit network interface before it can seed setup."
                        .into(),
                ),
            ),
            IpAddr::V6(ip) if (ip.segments()[0] & 0xffc0) == 0xfe80 && index.unwrap_or(0) == 0 => (
                None,
                Some("IPv6 scope is unavailable for this address.".into()),
            ),
            IpAddr::V4(ip) => (Some(ip.to_string()), None),
            IpAddr::V6(ip) => (
                Some(if index.unwrap_or(0) > 0 {
                    format!("[{ip}%{}]", index.unwrap())
                } else {
                    format!("[{ip}]")
                }),
                None,
            ),
        }
    };
    TvObservation {
        source: kind,
        friendly_name: None,
        model: None,
        instance: instance.into(),
        address: ip.to_string(),
        interface_name: name,
        interface_index: index,
        scope_id: scope,
        setup_host: endpoint,
        observed_age_ms: 0,
        cached: false,
        warning,
    }
}
fn display_label(value: &str) -> Option<String> {
    let value = value.trim();
    // Network observations are untrusted, bounded plain text. Reject controls and
    // bidi overrides instead of disguising them through lossy sanitization.
    if value.is_empty()
        || value.len() > 320
        || value.chars().count() > 80
        || value.chars().any(|c| {
            c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
    {
        return None;
    }
    Some(value.into())
}
fn metadata_labels(
    service: &mdns_sd::ResolvedService,
    kind: TvSource,
) -> (Option<String>, Option<String>) {
    match kind {
        TvSource::GoogleCast => {
            let video = service
                .txt_properties
                .get_property_val_str("ca")
                .filter(|value| value.len() <= 10)
                .and_then(|value| value.parse::<u32>().ok())
                .is_some_and(|capabilities| capabilities & 1 != 0);
            if !video {
                return (None, None);
            }
            (
                service
                    .txt_properties
                    .get_property_val_str("fn")
                    .and_then(display_label),
                service
                    .txt_properties
                    .get_property_val_str("md")
                    .and_then(display_label),
            )
        }
        TvSource::AndroidTvRemote => {
            let prefix = &service.fullname[..service.fullname.len() - service.ty_domain.len()];
            (
                display_label(prefix.trim_end_matches('.').replace("\\032", " ").as_str()),
                None,
            )
        }
    }
}
fn resolved(service: &mdns_sd::ResolvedService, observed: Instant) -> Option<DiscoveryEvent> {
    let kind = kind(&service.ty_domain)?;
    if service.port == 0
        || service.fullname.len() <= service.ty_domain.len()
        || service.fullname.len() > 255
        || service.fullname.chars().any(char::is_control)
        || !service
            .fullname
            .to_ascii_lowercase()
            .ends_with(&service.ty_domain.to_ascii_lowercase())
    {
        return None;
    }
    let mut rows = Vec::new();
    for address in service.addresses.iter().take(64) {
        match address {
            ScopedIp::V4(v4) => {
                let ids = v4.interface_ids();
                if ids.is_empty() {
                    rows.push(row(
                        kind,
                        &service.fullname,
                        IpAddr::V4(*v4.addr()),
                        None,
                        None,
                        false,
                    ));
                }
                for id in ids.iter().take(8) {
                    rows.push(row(
                        kind,
                        &service.fullname,
                        IpAddr::V4(*v4.addr()),
                        interface_name(&id.name),
                        (id.index != 0).then_some(id.index),
                        ids.len() > 1,
                    ));
                }
            }
            ScopedIp::V6(v6) => {
                let id = v6.scope_id();
                rows.push(row(
                    kind,
                    &service.fullname,
                    IpAddr::V6(*v6.addr()),
                    interface_name(&id.name),
                    (id.index != 0).then_some(id.index),
                    false,
                ));
            }
            _ => {}
        }
    }
    let (friendly_name, model) = metadata_labels(service, kind);
    if friendly_name.is_none() {
        rows.clear();
    }
    for row in &mut rows {
        row.friendly_name = friendly_name.clone();
        row.model = model.clone();
    }
    rows.sort_by_key(|r| (r.address.clone(), r.scope_id, r.interface_index));
    rows.dedup();
    rows.truncate(64);
    Some(DiscoveryEvent::Resolved {
        kind,
        instance: service.fullname.clone(),
        rows,
        observed,
    })
}
fn translate(event: ServiceEvent, observed: Instant) -> Option<DiscoveryEvent> {
    match event {
        ServiceEvent::ServiceResolved(service) => resolved(&service, observed),
        ServiceEvent::ServiceRemoved(service, instance) => {
            kind(&service).map(|kind| DiscoveryEvent::Removed { kind, instance })
        }
        _ => None,
    }
}

fn shutdown(daemon: &ServiceDaemon) -> bool {
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        match daemon.shutdown() {
            Ok(receiver) => {
                return matches!(
                    receiver.recv_timeout(end.saturating_duration_since(Instant::now())),
                    Ok(DaemonStatus::Shutdown)
                )
            }
            Err(mdns_sd::Error::DaemonShutdown) => return true,
            Err(mdns_sd::Error::Again) if Instant::now() < end => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(_) => return false,
        }
    }
}
fn collect_window(
    batch: &mut DiscoveryBatch,
    cancel: &AtomicBool,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
    mut poll: impl FnMut(&mut DiscoveryBatch, Instant) -> bool,
) {
    let end = now() + WINDOW;
    loop {
        let observed = now();
        if observed >= end || cancel.load(Ordering::Acquire) || batch.events.len() >= MAX_EVENTS {
            break;
        }
        if !poll(batch, observed) {
            sleep(Duration::from_millis(10).min(end.saturating_duration_since(observed)));
        }
    }
}
impl DiscoveryProvider for MdnsProvider {
    fn collect(&self, cancel: &AtomicBool) -> DiscoveryBatch {
        let mut batch = DiscoveryBatch {
            events: Vec::new(),
            warnings: Vec::new(),
            cleanup_confirmed: true,
        };
        // This is the only daemon creation point; constructing a driver is passive.
        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(error) => {
                batch
                    .warnings
                    .push(format!("TV name mDNS discovery could not start: {error}"));
                return batch;
            }
        };
        *self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(daemon.clone());
        let monitor = match daemon.monitor() {
            Ok(receiver) => Some(receiver),
            Err(error) => {
                batch
                    .warnings
                    .push(format!("mDNS diagnostics unavailable: {error}"));
                None
            }
        };
        let mut receivers = Vec::new();
        for (name, _) in SERVICES {
            match daemon.browse(name) {
                Ok(receiver) => receivers.push(receiver),
                Err(error) => batch
                    .warnings
                    .push(format!("Could not browse {name}: {error}")),
            }
        }
        collect_window(
            &mut batch,
            cancel,
            Instant::now,
            std::thread::sleep,
            |batch, observed| {
                let mut received = false;
                if let Some(monitor) = &monitor {
                    if let Ok(event) = monitor.try_recv() {
                        received = true;
                        if let DaemonEvent::Error(error) = event {
                            if batch.warnings.len() < 8 {
                                batch
                                    .warnings
                                    .push(format!("mDNS discovery error: {error}"));
                            }
                        }
                    }
                }
                for receiver in &receivers {
                    if let Ok(event) = receiver.try_recv() {
                        received = true;
                        if let ServiceEvent::ServiceResolved(service) = &event {
                            let rows = service
                                .addresses
                                .iter()
                                .take(64)
                                .map(|address| match address {
                                    ScopedIp::V4(v4) => v4.interface_ids().len().max(1),
                                    ScopedIp::V6(_) => 1,
                                    _ => 0,
                                })
                                .sum::<usize>();
                            let interfaces_limited = service.addresses.iter().take(64).any(|address| matches!(address, ScopedIp::V4(v4) if v4.interface_ids().len() > 8));
                            if (service.addresses.len() > 64 || rows > 64 || interfaces_limited)
                                && batch.warnings.len() < 8
                            {
                                batch.warnings.push("An advertisement exceeded the address limit; results are incomplete.".into());
                            }
                        }
                        if let Some(event) = translate(event, observed) {
                            batch.events.push(event);
                        }
                    }
                }
                received
            },
        );
        if batch.events.len() >= MAX_EVENTS {
            batch
                .warnings
                .push("Discovery event limit reached; results are incomplete.".into());
        }
        // mdns-sd can block sending to its bounded event channels.
        drop(receivers);
        drop(monitor);
        for (name, _) in SERVICES {
            let _ = daemon.stop_browse(name);
        }
        batch.cleanup_confirmed = shutdown(&daemon);
        if batch.cleanup_confirmed {
            *self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        } else {
            *self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(daemon);
        }
        batch
    }
    fn retry_cleanup(&self) -> bool {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.as_ref().map_or(true, shutdown) {
            *pending = None;
            true
        } else {
            false
        }
    }
}

// One passive owner across requests and subprocess-driver replacements.
// Every bounded browse shuts its daemon down before returning. No ADB driver is involved.
static OBSERVATIONS: std::sync::LazyLock<Arc<DiscoveryManager>> =
    std::sync::LazyLock::new(|| DiscoveryManager::new(Arc::new(MdnsProvider::default())));
pub(crate) async fn observe_names() -> AdbResult<TvDiscoveryReport> {
    if OBSERVATIONS.cleanup_pending() && !OBSERVATIONS.retry_cleanup().await {
        return Err(AdbError::Transport(
            "Previous discovery cleanup is still unconfirmed. Reopen the app before trying again."
                .into(),
        ));
    }
    let lease = Arc::new(
        super::child_leases()
            .try_acquire()
            .ok_or(AdbError::ShuttingDown)?,
    );
    OBSERVATIONS.snapshot(lease).await
}
pub(crate) fn cancel_observations() {
    OBSERVATIONS.cancel();
}

struct Cached {
    rows: Vec<TvObservation>,
    observed: Instant,
    cached: bool,
}
#[derive(Default)]
struct Cache {
    services: BTreeMap<Key, Cached>,
}
impl Cache {
    fn apply(&mut self, events: Vec<DiscoveryEvent>, now: Instant) -> bool {
        self.services
            .retain(|_, entry| now.saturating_duration_since(entry.observed) < MAX_AGE);
        for entry in self.services.values_mut() {
            entry.cached = true;
        }
        let mut truncated = false;
        for event in events.into_iter().take(MAX_EVENTS) {
            match event {
                DiscoveryEvent::Removed { kind, instance } => {
                    self.services.remove(&(kind, instance.to_ascii_lowercase()));
                }
                DiscoveryEvent::Resolved {
                    kind,
                    instance,
                    mut rows,
                    observed,
                } => {
                    if instance.len() > 255 || now.saturating_duration_since(observed) >= MAX_AGE {
                        continue;
                    }
                    let key = (kind, instance.to_ascii_lowercase());
                    self.services.remove(&key);
                    rows.truncate(64);
                    while self
                        .services
                        .values()
                        .map(|entry| entry.rows.len())
                        .sum::<usize>()
                        + rows.len()
                        > MAX_ROWS
                    {
                        let Some(oldest) = self
                            .services
                            .iter()
                            .min_by_key(|(_, entry)| entry.observed)
                            .map(|(key, _)| key.clone())
                        else {
                            break;
                        };
                        self.services.remove(&oldest);
                        truncated = true;
                    }
                    if !rows.is_empty() {
                        self.services.insert(
                            key,
                            Cached {
                                rows,
                                observed,
                                cached: false,
                            },
                        );
                    }
                }
            }
        }
        truncated
    }
    fn report(&self, now: Instant, mut warnings: Vec<String>) -> TvDiscoveryReport {
        let mut observations = Vec::new();
        for entry in self.services.values() {
            let age = now.saturating_duration_since(entry.observed);
            if age >= MAX_AGE {
                continue;
            }
            for row in &entry.rows {
                let mut row = row.clone();
                row.observed_age_ms = u64::try_from(age.as_millis()).unwrap_or(u64::MAX);
                row.cached = entry.cached;
                if row.cached {
                    row.setup_host = None;
                    row.warning =
                        Some("Cached observation was not confirmed in this browse window.".into());
                }
                observations.push(row);
            }
        }
        warnings.truncate(8);
        for warning in &mut warnings {
            *warning = warning.chars().take(240).collect();
        }
        TvDiscoveryReport {
            observations,
            warnings,
            observation_max_age_ms: 30_000,
        }
    }
}

pub(crate) struct DiscoveryManager {
    provider: Arc<dyn DiscoveryProvider>,
    cache: Mutex<Cache>,
    gate: Arc<tokio::sync::Mutex<()>>,
    current: Mutex<Option<Arc<AtomicBool>>>,
    // Unknown daemon cleanup stays counted even if its driver is dropped.
    pending_lease: Mutex<Option<ManuallyDrop<Arc<Lease>>>>,
    unrecoverable: AtomicBool,
    closed: AtomicBool,
}
impl DiscoveryManager {
    pub fn new(provider: Arc<dyn DiscoveryProvider>) -> Arc<Self> {
        Arc::new(Self {
            provider,
            cache: Mutex::new(Cache::default()),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            current: Mutex::new(None),
            pending_lease: Mutex::new(None),
            unrecoverable: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        })
    }
    pub fn cancel(&self) {
        let current = self
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.closed.store(true, Ordering::Release);
        if let Some(signal) = current.as_ref() {
            signal.store(true, Ordering::Release);
        }
    }
    pub fn cleanup_pending(&self) -> bool {
        self.pending_lease
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }
    pub async fn retry_cleanup(self: &Arc<Self>) -> bool {
        if self.unrecoverable.load(Ordering::Acquire) {
            return false;
        }
        let manager = self.clone();
        tokio::task::spawn_blocking(move || {
            let _operation = manager
                .pending_lease
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .map(|lease| Arc::clone(lease));
            if manager.provider.retry_cleanup() {
                if let Some(lease) = manager
                    .pending_lease
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    drop(ManuallyDrop::into_inner(lease));
                }
                true
            } else {
                false
            }
        })
        .await
        .unwrap_or(false)
    }
    pub async fn snapshot(self: &Arc<Self>, lease: Arc<Lease>) -> AdbResult<TvDiscoveryReport> {
        if self.cleanup_pending() {
            return Err(AdbError::Transport("TV name discovery cleanup is unconfirmed. Additional discovery is blocked; manual connection remains available.".into()));
        }
        let gate = self.gate.clone().lock_owned().await;
        if self.cleanup_pending() {
            return Err(AdbError::Transport(
                "TV name discovery cleanup is still pending.".into(),
            ));
        }
        let signal = Arc::new(AtomicBool::new(false));
        {
            let mut current = self
                .current
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if self.closed.load(Ordering::Acquire) {
                return Err(AdbError::ShuttingDown);
            }
            *current = Some(signal.clone());
        }
        struct Cancel(Arc<AtomicBool>);
        impl Drop for Cancel {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _cancel = Cancel(signal.clone());
        let manager = self.clone();
        tokio::task::spawn_blocking(move || {
            let _gate = gate;
            let batch = if signal.load(Ordering::Acquire) {
                DiscoveryBatch {
                    events: Vec::new(),
                    warnings: vec!["Discovery was cancelled before browsing.".into()],
                    cleanup_confirmed: true,
                }
            } else {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    manager.provider.collect(&signal)
                })) {
                    Ok(batch) => batch,
                    Err(_) => {
                        manager.unrecoverable.store(true, Ordering::Release);
                        DiscoveryBatch {
                            events: Vec::new(),
                            warnings: vec![
                            "TV name discovery failed unexpectedly; cleanup cannot be confirmed."
                                .into(),
                        ],
                            cleanup_confirmed: false,
                        }
                    }
                }
            };
            let mut warnings = batch.warnings;
            if !batch.cleanup_confirmed {
                warnings.insert(
                    0,
                    "TV name discovery shutdown was not confirmed; additional discovery is blocked."
                        .into(),
                );
                *manager
                    .pending_lease
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some(ManuallyDrop::new(lease.clone()));
            }
            let now = Instant::now();
            let mut cache = manager
                .cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if cache.apply(batch.events, now) {
                warnings
                    .push("Discovery cache limit reached; some observations were omitted.".into());
            }
            *manager
                .current
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            cache.report(now, warnings)
        })
        .await
        .map_err(|error| AdbError::Transport(format!("TV name discovery worker failed: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_optimizer_core::adb::Leases;
    use std::collections::VecDeque;

    fn advertisement(kind: TvSource, name: &str, address: &str, _port: u16) -> TvObservation {
        row(kind, name, address.parse().unwrap(), None, None, false)
    }
    fn event(
        kind: TvSource,
        name: &str,
        address: &str,
        port: u16,
        observed: Instant,
    ) -> DiscoveryEvent {
        DiscoveryEvent::Resolved {
            kind,
            instance: name.into(),
            rows: vec![advertisement(kind, name, address, port)],
            observed,
        }
    }
    fn batch(events: Vec<DiscoveryEvent>) -> DiscoveryBatch {
        DiscoveryBatch {
            events,
            warnings: Vec::new(),
            cleanup_confirmed: true,
        }
    }
    #[derive(Default)]
    struct FakeProvider {
        batches: Mutex<VecDeque<DiscoveryBatch>>,
        calls: std::sync::atomic::AtomicUsize,
        clean: AtomicBool,
    }
    impl DiscoveryProvider for FakeProvider {
        fn collect(&self, _: &AtomicBool) -> DiscoveryBatch {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.batches
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| batch(Vec::new()))
        }
        fn retry_cleanup(&self) -> bool {
            self.clean.load(Ordering::Acquire)
        }
    }

    #[test]
    fn collection_window_includes_delayed_advertisement_and_stays_bounded() {
        use std::cell::Cell;
        let start = Instant::now();
        let elapsed = Cell::new(Duration::ZERO);
        let cancel = AtomicBool::new(false);
        let mut batch = DiscoveryBatch {
            events: vec![],
            warnings: vec![],
            cleanup_confirmed: true,
        };
        let mut emitted = false;
        collect_window(
            &mut batch,
            &cancel,
            || start + elapsed.get(),
            |duration| elapsed.set(elapsed.get() + duration),
            |batch, observed| {
                if !emitted && observed.duration_since(start) >= Duration::from_millis(3500) {
                    emitted = true;
                    batch.events.push(DiscoveryEvent::Resolved {
                        kind: TvSource::GoogleCast,
                        instance: "delayed._googlecast._tcp.local.".into(),
                        rows: vec![row(
                            TvSource::GoogleCast,
                            "delayed._googlecast._tcp.local.",
                            "fe80::1234".parse().unwrap(),
                            Some("en0".into()),
                            Some(7),
                            false,
                        )],
                        observed,
                    });
                    true
                } else {
                    false
                }
            },
        );
        assert!(
            emitted,
            "observations arriving after two seconds must be collected"
        );
        assert_eq!(batch.events.len(), 1);
        let DiscoveryEvent::Resolved { rows, observed, .. } = &batch.events[0] else {
            panic!("expected delayed advertisement");
        };
        assert_eq!(observed.duration_since(start), Duration::from_millis(3500));
        assert_eq!(rows[0].setup_host.as_deref(), Some("[fe80::1234%7]"));
        assert_eq!(rows[0].interface_name.as_deref(), Some("en0"));
        assert_eq!(elapsed.get(), Duration::from_secs(6));
    }

    #[test]
    fn collection_window_respects_cancellation_and_event_cap() {
        use std::cell::Cell;
        for capped in [false, true] {
            let start = Instant::now();
            let elapsed = Cell::new(Duration::ZERO);
            let cancel = AtomicBool::new(false);
            let mut batch = DiscoveryBatch {
                events: vec![],
                warnings: vec![],
                cleanup_confirmed: true,
            };
            let mut polls = 0;
            collect_window(
                &mut batch,
                &cancel,
                || start + elapsed.get(),
                |duration| elapsed.set(elapsed.get() + duration),
                |batch, _| {
                    polls += 1;
                    if capped {
                        for _ in 0..MAX_EVENTS {
                            batch.events.push(DiscoveryEvent::Removed {
                                kind: TvSource::GoogleCast,
                                instance: "test".into(),
                            });
                        }
                    } else {
                        cancel.store(true, Ordering::Release);
                    }
                    true
                },
            );
            assert_eq!(polls, 1);
            assert_eq!(elapsed.get(), Duration::ZERO);
        }
    }

    fn metadata_service(
        kind: &str,
        name: &str,
        properties: &[(&str, &str)],
    ) -> mdns_sd::ResolvedService {
        mdns_sd::ServiceInfo::new(kind, name, "fixture.local.", "192.0.2.42", 8009, properties)
            .unwrap()
            .as_resolved_service()
    }
    #[test]
    fn metadata_labels_never_expose_service_ports_or_adb_endpoints() {
        for service in [
            metadata_service(
                "_googlecast._tcp.local.",
                "cast-fixture",
                &[("fn", "Bedroom TV"), ("md", "Fixture TV"), ("ca", "5")],
            ),
            metadata_service("_androidtvremote2._tcp.local.", "Bedroom TV", &[]),
        ] {
            let DiscoveryEvent::Resolved { rows, .. } = resolved(&service, Instant::now()).unwrap()
            else {
                panic!("resolved");
            };
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].friendly_name.as_deref(), Some("Bedroom TV"));
            assert_eq!(rows[0].setup_host.as_deref(), Some("192.0.2.42"));
            let json = serde_json::to_value(&rows[0]).unwrap();
            for forbidden in ["port", "connect_address", "hardware_id", "serial"] {
                assert!(json.get(forbidden).is_none());
            }
        }
        assert!(kind("_adb._tcp.local.").is_none());
        assert!(kind("_adb-tls-connect._tcp.local.").is_none());
    }
    #[test]
    fn invalid_audio_only_and_hostile_names_are_withheld() {
        for props in [
            vec![("fn", "Speaker"), ("ca", "4")],
            vec![("fn", "TV")],
            vec![("ca", "1")],
            vec![("fn", "TV\u{202e}"), ("ca", "1")],
            vec![("fn", "TV"), ("ca", "-1")],
        ] {
            let service = metadata_service("_googlecast._tcp.local.", "fixture", &props);
            let DiscoveryEvent::Resolved { rows, .. } = resolved(&service, Instant::now()).unwrap()
            else {
                panic!("resolved");
            };
            assert!(rows.is_empty());
        }
        assert!(display_label(&"x".repeat(81)).is_none());
        assert!(display_label("bad\nlabel").is_none());
        assert_eq!(
            display_label("  Guest room TV  ").as_deref(),
            Some("Guest room TV")
        );
    }
    #[test]
    fn metadata_goodbye_and_invalid_replacement_remove_old_name() {
        let now = Instant::now();
        let good = metadata_service(
            "_googlecast._tcp.local.",
            "fixture",
            &[("fn", "TV"), ("ca", "1")],
        );
        let invalid = metadata_service(
            "_googlecast._tcp.local.",
            "fixture",
            &[("fn", "Speaker"), ("ca", "4")],
        );
        let mut cache = Cache::default();
        cache.apply(vec![resolved(&good, now).unwrap()], now);
        assert_eq!(cache.report(now, vec![]).observations.len(), 1);
        cache.apply(vec![resolved(&invalid, now).unwrap()], now);
        assert!(cache.report(now, vec![]).observations.is_empty());
        cache.apply(vec![resolved(&good, now).unwrap()], now);
        cache.apply(
            vec![translate(
                ServiceEvent::ServiceRemoved(good.ty_domain.clone(), good.fullname.clone()),
                now,
            )
            .unwrap()],
            now,
        );
        assert!(cache.report(now, vec![]).observations.is_empty());
    }
    #[test]
    fn unusable_or_ambiguous_interfaces_cannot_seed_manual_setup() {
        for ip in ["127.0.0.1", "0.0.0.0", "224.0.0.1", "169.254.1.2"] {
            assert!(row(
                TvSource::GoogleCast,
                "fixture",
                ip.parse().unwrap(),
                None,
                None,
                false
            )
            .setup_host
            .is_none());
        }
        assert!(row(
            TvSource::GoogleCast,
            "fixture",
            "192.0.2.42".parse().unwrap(),
            None,
            Some(7),
            true
        )
        .setup_host
        .is_none());
        assert!(row(
            TvSource::GoogleCast,
            "fixture",
            "fe80::42".parse().unwrap(),
            None,
            None,
            false
        )
        .setup_host
        .is_none());
        let scoped = row(
            TvSource::AndroidTvRemote,
            "fixture",
            "fe80::42".parse().unwrap(),
            Some("fixture0".into()),
            Some(7),
            false,
        );
        assert_eq!(scoped.setup_host.as_deref(), Some("[fe80::42%7]"));
        assert_eq!(scoped.interface_name.as_deref(), Some("fixture0"));
    }

    #[test]
    fn discovery_cache_and_warning_sizes_are_bounded() {
        let now = Instant::now();
        let mut cache = Cache::default();
        let events = (0..300)
            .map(|i| {
                event(
                    TvSource::GoogleCast,
                    &format!("instance-{i}"),
                    "192.0.2.1",
                    39001,
                    now,
                )
            })
            .collect();
        assert!(cache.apply(events, now));
        let report = cache.report(now, vec!["x".repeat(1000); 100]);
        assert_eq!(report.observations.len(), MAX_ROWS);
        assert_eq!(report.warnings.len(), 8);
        assert!(report.warnings.iter().all(|w| w.chars().count() <= 240));
    }

    #[tokio::test]
    async fn discovery_manager_returns_usable_fresh_rows_and_requires_each_scan_to_reobserve() {
        let provider = Arc::new(FakeProvider::default());
        let manager = DiscoveryManager::new(provider.clone());
        let leases = Leases::new();
        let kind = TvSource::GoogleCast;
        let name = "Living Room._googlecast._tcp.local.";
        provider.batches.lock().unwrap().push_back(batch(vec![event(
            kind,
            name,
            "192.0.2.71",
            5555,
            Instant::now() - WINDOW,
        )]));
        let first = manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .unwrap();
        assert_eq!(first.observation_max_age_ms, 30_000);
        assert_eq!(first.observations.len(), 1);
        let fresh = &first.observations[0];
        assert!(!fresh.cached);
        assert_eq!(fresh.setup_host.as_deref(), Some("192.0.2.71"));
        assert!(fresh.observed_age_ms >= WINDOW.as_millis() as u64);
        assert!(fresh.observed_age_ms < first.observation_max_age_ms);
        let serialized = serde_json::to_value(&first).unwrap();
        assert_eq!(serialized["observation_max_age_ms"], 30_000);
        assert_eq!(
            serialized["observations"][0]["observed_age_ms"],
            fresh.observed_age_ms
        );

        let unconfirmed = manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .unwrap();
        assert_eq!(unconfirmed.observations.len(), 1);
        assert!(unconfirmed.observations[0].cached);
        assert!(unconfirmed.observations[0].setup_host.is_none());
        assert!(unconfirmed.observations[0].observed_age_ms >= fresh.observed_age_ms);

        provider.batches.lock().unwrap().push_back(batch(vec![event(
            kind,
            name,
            "192.0.2.71",
            5555,
            Instant::now(),
        )]));
        let renewed = manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .unwrap();
        assert_eq!(renewed.observations.len(), 1);
        assert!(!renewed.observations[0].cached);
        assert_eq!(
            renewed.observations[0].setup_host.as_deref(),
            Some("192.0.2.71")
        );
        assert!(renewed.observations[0].observed_age_ms < renewed.observation_max_age_ms);

        // Advance only the injected observation's age, without sleeping or browsing the LAN.
        manager
            .cache
            .lock()
            .unwrap()
            .services
            .get_mut(&(kind, name.to_ascii_lowercase()))
            .unwrap()
            .observed = Instant::now() - MAX_AGE;
        let expired = manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .unwrap();
        assert!(expired.observations.is_empty());
        assert_eq!(provider.calls.load(Ordering::Relaxed), 4);
        assert_eq!(leases.active(), 0);
    }

    #[tokio::test]
    async fn discovery_provider_failure_returns_only_disabled_cache_observations() {
        let provider = Arc::new(FakeProvider::default());
        let now = Instant::now();
        provider.batches.lock().unwrap().push_back(batch(vec![event(
            TvSource::GoogleCast,
            "one",
            "192.0.2.1",
            39001,
            now,
        )]));
        provider.batches.lock().unwrap().push_back(DiscoveryBatch {
            events: Vec::new(),
            warnings: vec!["fixture startup failed".into()],
            cleanup_confirmed: true,
        });
        let manager = DiscoveryManager::new(provider);
        let leases = Leases::new();
        assert!(
            !manager
                .snapshot(Arc::new(leases.try_acquire().unwrap()))
                .await
                .unwrap()
                .observations[0]
                .cached
        );
        let failed = manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .unwrap();
        assert!(failed.observations[0].cached);
        assert!(failed.observations[0].setup_host.is_none());
        assert!(failed.warnings.iter().any(|w| w.contains("startup failed")));
        assert_eq!(leases.active(), 0);
    }

    #[tokio::test]
    async fn discovery_unconfirmed_cleanup_retains_lease_and_blocks_additional_daemons() {
        let provider = Arc::new(FakeProvider::default());
        provider.batches.lock().unwrap().push_back(DiscoveryBatch {
            events: Vec::new(),
            warnings: Vec::new(),
            cleanup_confirmed: false,
        });
        let manager = DiscoveryManager::new(provider.clone());
        let leases = Leases::new();
        let report = manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .unwrap();
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("shutdown was not confirmed")));
        assert_eq!(leases.active(), 1);
        assert!(manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .is_err());
        assert_eq!(provider.calls.load(Ordering::Relaxed), 1);
        assert!(!manager.retry_cleanup().await);
        assert_eq!(leases.active(), 1);
        provider.clean.store(true, Ordering::Release);
        assert!(manager.retry_cleanup().await);
        assert_eq!(leases.active(), 0);
    }

    #[tokio::test]
    async fn discovery_cancelled_worker_remains_counted_until_provider_cleanup_finishes() {
        struct Held {
            started: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
            release: Mutex<std::sync::mpsc::Receiver<()>>,
        }
        impl DiscoveryProvider for Held {
            fn collect(&self, cancel: &AtomicBool) -> DiscoveryBatch {
                self.started
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap()
                    .send(())
                    .unwrap();
                while !cancel.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(1));
                }
                self.release.lock().unwrap().recv().unwrap();
                batch(Vec::new())
            }
            fn retry_cleanup(&self) -> bool {
                true
            }
        }
        let (started, started_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let leases = Leases::new();
        let manager = DiscoveryManager::new(Arc::new(Held {
            started: Mutex::new(Some(started)),
            release: Mutex::new(release_rx),
        }));
        let worker = tokio::spawn({
            let manager = manager.clone();
            let lease = Arc::new(leases.try_acquire().unwrap());
            async move { manager.snapshot(lease).await }
        });
        started_rx.await.unwrap();
        worker.abort();
        assert!(worker.await.unwrap_err().is_cancelled());
        assert_eq!(leases.active(), 1);
        leases.close();
        assert!(leases.try_acquire().is_none());
        release.send(()).unwrap();
        assert!(
            leases
                .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
                .await
        );
    }
    #[tokio::test]
    async fn discovery_provider_panic_cannot_release_unknown_work_or_start_another_daemon() {
        struct Panics;
        impl DiscoveryProvider for Panics {
            fn collect(&self, _: &AtomicBool) -> DiscoveryBatch {
                panic!("expected fixture panic before any real daemon");
            }
            fn retry_cleanup(&self) -> bool {
                panic!("unknown cleanup must not be guessed");
            }
        }
        let manager = DiscoveryManager::new(Arc::new(Panics));
        let leases = Leases::new();
        manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .unwrap();
        assert_eq!(leases.active(), 1);
        assert!(!manager.retry_cleanup().await);
        assert!(manager
            .snapshot(Arc::new(leases.try_acquire().unwrap()))
            .await
            .is_err());
        // This fake owns no daemon; release its simulated token after the assertions.
        let retained = manager.pending_lease.lock().unwrap().take().unwrap();
        drop(ManuallyDrop::into_inner(retained));
        assert_eq!(leases.active(), 0);
    }
    #[tokio::test]
    async fn discovery_queued_before_shutdown_does_not_start_after_admission_closes() {
        let provider = Arc::new(FakeProvider::default());
        let manager = DiscoveryManager::new(provider.clone());
        let leases = Leases::new();
        let gate = manager.gate.clone().lock_owned().await;
        let worker = tokio::spawn({
            let manager = manager.clone();
            let lease = Arc::new(leases.try_acquire().unwrap());
            async move { manager.snapshot(lease).await }
        });
        tokio::task::yield_now().await;
        manager.cancel();
        leases.close();
        drop(gate);
        assert!(matches!(worker.await.unwrap(), Err(AdbError::ShuttingDown)));
        assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
        assert_eq!(leases.active(), 0);
    }
}
