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
    if let Err(e) = tokio::fs::write(&path, text).await {
        warn!(path = %path.display(), error = %e, "could not write tracking file");
    }
}

/// Tracked packages for `serial` that are still in the device's disabled
/// list. Entries that aren't (re-enabled or uninstalled out-of-band) are
/// dropped and the file rewritten.
pub async fn prune(data_dir: &Path, serial: &str, disabled_pkgs: &[String]) -> Vec<String> {
    let mut map = load_map(data_dir).await;
    let Some(tracked) = map.get_mut(serial) else {
        return Vec::new();
    };
    let before = tracked.len();
    tracked.retain(|pkg| disabled_pkgs.iter().any(|d| d == pkg));
    let current = tracked.clone();
    if tracked.len() != before {
        if tracked.is_empty() {
            map.remove(serial);
        }
        save_map(data_dir, &map).await;
    }
    current
}

/// Record that `package` was disabled on `serial` via the Launchers list.
pub async fn record(data_dir: &Path, serial: &str, package: &str) {
    let mut map = load_map(data_dir).await;
    let tracked = map.entry(serial.to_string()).or_default();
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

    #[tokio::test]
    async fn record_then_prune_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        record(dir.path(), "serial-1", "com.example.home").await;
        record(dir.path(), "serial-1", "com.example.home").await; // idempotent
        record(dir.path(), "serial-2", "com.other.home").await;

        // Still disabled → kept; per-serial isolation.
        let kept = prune(dir.path(), "serial-1", &pkgs(&["com.example.home"])).await;
        assert_eq!(kept, pkgs(&["com.example.home"]));

        // Re-enabled out-of-band → dropped.
        let kept = prune(dir.path(), "serial-1", &[]).await;
        assert_eq!(kept, Vec::<String>::new());

        // serial-2 untouched by serial-1's prune.
        let kept = prune(dir.path(), "serial-2", &pkgs(&["com.other.home"])).await;
        assert_eq!(kept, pkgs(&["com.other.home"]));
    }

    #[tokio::test]
    async fn unreadable_or_missing_file_degrades_to_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(prune(dir.path(), "s", &[]).await, Vec::<String>::new());

        tokio::fs::write(tracking_path(dir.path()), "not json")
            .await
            .unwrap();
        assert_eq!(prune(dir.path(), "s", &[]).await, Vec::<String>::new());
    }
}
