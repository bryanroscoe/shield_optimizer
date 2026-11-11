//! Remembers non-catalog HOME handlers the user disabled from the Launchers
//! list. Disabled packages don't answer the HOME intent query, so without
//! this file their rows — and the one-click Enable path back — would vanish
//! the moment they're disabled. Stock/custom catalog launchers don't need
//! tracking (they're detected via `pm list packages`, which includes
//! disabled apps).
//!
//! Best-effort persistence: a read or write failure degrades to "row not
//! shown", never to a command error. Stale entries (re-enabled or
//! uninstalled out-of-band) are pruned by `prune` on every launcher-list
//! load.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tracing::warn;

type TrackingMap = BTreeMap<String, Vec<String>>;

/// The TV a tracker entry belongs to. Entries are keyed by the verified
/// hardware id when the TV reports one, so a TV keeps its entries across
/// address changes and another TV that later takes the address never
/// inherits them. A TV with no readable id falls back to its transport
/// serial, which is also how entries were keyed before.
#[derive(Debug, Clone, Copy)]
pub struct TrackedDevice<'a> {
    pub serial: &'a str,
    /// `ro.serialno`, raw; checked with `verified_hardware_id`.
    pub hardware_id: Option<&'a str>,
}

impl TrackedDevice<'_> {
    fn verified_id(&self) -> Option<&str> {
        self.hardware_id
            .and_then(super::devices::verified_hardware_id)
    }

    fn key(&self) -> String {
        match self.verified_id() {
            Some(id) => format!("hw:{id}"),
            None => self.serial.to_string(),
        }
    }
}

/// One load-modify-save at a time, so concurrent records never drop each
/// other's entries.
fn file_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn tracking_path(data_dir: &Path) -> PathBuf {
    data_dir.join("disabled-home-handlers.json")
}

async fn load_map(data_dir: &Path) -> TrackingMap {
    let path = tracking_path(data_dir);
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            warn!(path = %path.display(), error = %e, "tracking file unreadable; starting fresh");
            TrackingMap::new()
        }),
        Err(_) => TrackingMap::new(), // Usually just "file doesn't exist yet".
    }
}

async fn save_map(data_dir: &Path, map: &TrackingMap) {
    let path = tracking_path(data_dir);
    let _ = tokio::fs::create_dir_all(data_dir).await;
    let text = match serde_json::to_string_pretty(map) {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, "could not serialize tracking map");
            return;
        }
    };
    let dir = data_dir.to_path_buf();
    let written = tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let mut temporary = tempfile::NamedTempFile::new_in(&dir)?;
        temporary.write_all(text.as_bytes())?;
        temporary.persist(&path).map(drop).map_err(|e| e.error)
    })
    .await;
    match written {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warn!(error = %e, "could not write tracking file"),
        Err(e) => warn!(error = %e, "could not write tracking file"),
    }
}

/// Tracked packages for `device` that are still in its disabled list.
/// Entries that aren't (re-enabled or uninstalled out-of-band) are dropped
/// and the file rewritten.
///
/// Entries recorded under this transport serial before the TV's hardware id
/// was known move to the id only when this TV confirms them by having that
/// package disabled; the rest stay where they are, since they may belong to
/// another TV that used the same address.
pub async fn prune(
    data_dir: &Path,
    device: TrackedDevice<'_>,
    disabled_pkgs: &[String],
) -> Vec<String> {
    let _guard = file_lock().lock().await;
    let mut map = load_map(data_dir).await;
    let key = device.key();
    let mut changed = false;
    if key != device.serial {
        if let Some(legacy) = map.get_mut(device.serial) {
            let (confirmed, rest): (Vec<String>, Vec<String>) = legacy
                .drain(..)
                .partition(|pkg| disabled_pkgs.iter().any(|d| d == pkg));
            *legacy = rest;
            if legacy.is_empty() {
                map.remove(device.serial);
            }
            if !confirmed.is_empty() {
                let tracked = map.entry(key.clone()).or_default();
                for pkg in confirmed {
                    if !tracked.contains(&pkg) {
                        tracked.push(pkg);
                    }
                }
                changed = true;
            }
        }
    }
    let current = match map.get_mut(&key) {
        Some(tracked) => {
            let before = tracked.len();
            tracked.retain(|pkg| disabled_pkgs.iter().any(|d| d == pkg));
            changed |= tracked.len() != before;
            let current = tracked.clone();
            if current.is_empty() {
                map.remove(&key);
            }
            current
        }
        None => Vec::new(),
    };
    if changed {
        save_map(data_dir, &map).await;
    }
    current
}

/// Record that `package` was disabled on `device` via the Launchers list.
pub async fn record(data_dir: &Path, device: TrackedDevice<'_>, package: &str) {
    let _guard = file_lock().lock().await;
    let mut map = load_map(data_dir).await;
    let tracked = map.entry(device.key()).or_default();
    if !tracked.iter().any(|p| p == package) {
        tracked.push(package.to_string());
        save_map(data_dir, &map).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn pkgs(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn at(serial: &str) -> TrackedDevice<'_> {
        TrackedDevice {
            serial,
            hardware_id: None,
        }
    }

    fn tv<'a>(serial: &'a str, id: &'a str) -> TrackedDevice<'a> {
        TrackedDevice {
            serial,
            hardware_id: Some(id),
        }
    }

    #[tokio::test]
    async fn entries_follow_the_hardware_id_not_the_address() {
        let dir = tempfile::tempdir().unwrap();
        let home = pkgs(&["com.example.home"]);
        record(dir.path(), tv("10.0.0.5:5555", "TV-A"), "com.example.home").await;
        // TV A at a new address keeps its entry.
        assert_eq!(
            prune(dir.path(), tv("10.0.0.9:5555", "TV-A"), &home).await,
            home
        );
        // TV B at A's old address does not inherit it.
        assert!(prune(dir.path(), tv("10.0.0.5:5555", "TV-B"), &home)
            .await
            .is_empty());
        // An unverifiable id falls back to the address.
        record(
            dir.path(),
            tv("10.0.0.7:5555", "unknown"),
            "com.example.home",
        )
        .await;
        assert_eq!(prune(dir.path(), at("10.0.0.7:5555"), &home).await, home);
    }

    #[tokio::test]
    async fn an_address_keyed_entry_moves_only_to_a_tv_that_confirms_it() {
        let dir = tempfile::tempdir().unwrap();
        record(dir.path(), at("10.0.0.5:5555"), "com.example.home").await;
        record(dir.path(), at("10.0.0.5:5555"), "com.example.other").await;
        // This TV has only one of them disabled: that one moves to its id.
        let kept = prune(
            dir.path(),
            tv("10.0.0.5:5555", "TV-A"),
            &pkgs(&["com.example.home"]),
        )
        .await;
        assert_eq!(kept, pkgs(&["com.example.home"]));
        let map = load_map(dir.path()).await;
        assert_eq!(map.get("hw:TV-A"), Some(&pkgs(&["com.example.home"])));
        // The unconfirmed one stays on the address, for whichever TV it was.
        assert_eq!(
            map.get("10.0.0.5:5555"),
            Some(&pkgs(&["com.example.other"]))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_records_keep_every_entry() {
        let dir = tempfile::tempdir().unwrap();
        let names: Vec<String> = (0..16).map(|i| format!("com.example.home{i}")).collect();
        let tasks: Vec<_> = names
            .iter()
            .map(|pkg| {
                let data_dir = dir.path().to_path_buf();
                let pkg = pkg.clone();
                tokio::spawn(async move { record(&data_dir, tv("s", "TV-A"), &pkg).await })
            })
            .collect();
        for task in tasks {
            task.await.unwrap();
        }
        let kept = prune(dir.path(), tv("s", "TV-A"), &names).await;
        assert_eq!(kept.len(), names.len(), "{kept:?}");
    }

    #[tokio::test]
    async fn record_then_prune_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        record(dir.path(), at("serial-1"), "com.example.home").await;
        record(dir.path(), at("serial-1"), "com.example.home").await; // idempotent
        record(dir.path(), at("serial-2"), "com.other.home").await;

        // Still disabled → kept; per-serial isolation.
        let kept = prune(dir.path(), at("serial-1"), &pkgs(&["com.example.home"])).await;
        assert_eq!(kept, pkgs(&["com.example.home"]));

        // Re-enabled out-of-band → dropped.
        let kept = prune(dir.path(), at("serial-1"), &[]).await;
        assert_eq!(kept, Vec::<String>::new());

        // serial-2 untouched by serial-1's prune.
        let kept = prune(dir.path(), at("serial-2"), &pkgs(&["com.other.home"])).await;
        assert_eq!(kept, pkgs(&["com.other.home"]));
    }

    #[tokio::test]
    async fn unreadable_or_missing_file_degrades_to_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(prune(dir.path(), at("s"), &[]).await, Vec::<String>::new());

        tokio::fs::write(tracking_path(dir.path()), "not json")
            .await
            .unwrap();
        assert_eq!(prune(dir.path(), at("s"), &[]).await, Vec::<String>::new());
    }
}
