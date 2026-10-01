//! Parsers for installed-app storage: `dumpsys diskstats` for the batched
//! read, and `pm path` + `stat -c %s` for a single package when diskstats has
//! no row for it.
//!
//! Every size is `Option`: a column the device did not report stays `None`
//! and is shown as unavailable, never as zero.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Installed storage for one package, in bytes.
///
/// diskstats reports `data_bytes` *including* the cache: on every row of the
/// captured Shield output data is at least the cache figure, and several rows
/// are almost entirely cache. The two are therefore never summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppStorage {
    pub app_bytes: Option<u64>,
    pub data_bytes: Option<u64>,
    pub cache_bytes: Option<u64>,
}

fn json_array_line<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })
}

fn size_column(output: &str, key: &str, len: usize) -> Option<Vec<Option<u64>>> {
    let raw = json_array_line(output, key)?;
    let values: Vec<serde_json::Value> = serde_json::from_str(raw).ok()?;
    // A column that does not line up with the names cannot be attributed to
    // any package, so it is dropped whole rather than guessed at.
    if values.len() != len {
        return None;
    }
    Some(
        values
            .iter()
            .map(|v| v.as_i64().and_then(|n| u64::try_from(n).ok()))
            .collect(),
    )
}

/// Per-package sizes from `dumpsys diskstats`.
///
/// The figures come from Android's own cache (`diskstats_cache.json`), which
/// the system refreshes on its own schedule, so they can be older than the
/// read. Returns `None` when the output has no package table at all — older
/// releases, or a device whose cache has not been written yet.
pub fn parse_diskstats_package_sizes(output: &str) -> Option<HashMap<String, AppStorage>> {
    let names: Vec<String> =
        serde_json::from_str(json_array_line(output, "Package Names")?).ok()?;
    if names.is_empty() {
        return None;
    }
    let len = names.len();
    let app = size_column(output, "App Sizes", len);
    let data = size_column(output, "App Data Sizes", len);
    let cache = size_column(output, "Cache Sizes", len);
    if app.is_none() && data.is_none() && cache.is_none() {
        return None;
    }
    let pick = |col: &Option<Vec<Option<u64>>>, i: usize| col.as_ref().and_then(|c| c[i]);
    Some(
        names
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                (
                    name,
                    AppStorage {
                        app_bytes: pick(&app, i),
                        data_bytes: pick(&data, i),
                        cache_bytes: pick(&cache, i),
                    },
                )
            })
            .collect(),
    )
}

/// APK paths from `pm path <pkg>`, keeping only paths safe to pass back to a
/// shell unquoted. A path with anything else in it is refused, which turns the
/// whole read into "unavailable" rather than a partial sum.
pub fn parse_pm_path_apks(output: &str) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    for line in output.lines() {
        let Some(path) = line.trim().strip_prefix("package:") else {
            continue;
        };
        let safe = path.starts_with('/')
            && path
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._~=+-".contains(c));
        if !safe {
            return None;
        }
        paths.push(path.to_string());
    }
    (!paths.is_empty()).then_some(paths)
}

/// Sum `stat -c %s` output for `expected` files. Anything short of one size
/// per file is a failed read, not a smaller app.
pub fn parse_stat_sizes_total(output: &str, expected: usize) -> Option<u64> {
    let sizes: Vec<u64> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if sizes.len() != expected || expected == 0 {
        return None;
    }
    sizes
        .into_iter()
        .try_fold(0u64, |acc, n| acc.checked_add(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIELD: &str = include_str!("fixtures/diskstats-shield-android11.txt");
    const NO_TABLE: &str = include_str!("fixtures/diskstats-no-package-cache.txt");
    const PM_PATH: &str = include_str!("fixtures/pm-path-youtube-tv-split.txt");
    const STAT: &str = include_str!("fixtures/stat-youtube-tv-split.txt");

    #[test]
    fn diskstats_reads_every_column_for_a_package() {
        let map = parse_diskstats_package_sizes(SHIELD).expect("table present");
        assert_eq!(map.len(), 12);
        assert_eq!(
            map["com.google.android.youtube.tv"],
            AppStorage {
                app_bytes: Some(63_782_912),
                data_bytes: Some(11_202_560),
                cache_bytes: Some(9_244_672),
            }
        );
        assert_eq!(map["android"].app_bytes, Some(34_562_048));
    }

    #[test]
    fn diskstats_data_includes_cache_on_real_output() {
        let map = parse_diskstats_package_sizes(SHIELD).unwrap();
        for (pkg, s) in &map {
            assert!(
                s.data_bytes.unwrap() >= s.cache_bytes.unwrap(),
                "{pkg}: data is reported inclusive of cache"
            );
        }
    }

    #[test]
    fn diskstats_without_a_package_table_is_unavailable() {
        assert_eq!(parse_diskstats_package_sizes(NO_TABLE), None);
        assert_eq!(parse_diskstats_package_sizes(""), None);
    }

    #[test]
    fn a_misaligned_column_is_dropped_not_guessed() {
        let text = "Package Names: [\"a.b\",\"c.d\"]\nApp Sizes: [10,20]\nApp Data Sizes: [5]\nCache Sizes: [1,2]\n";
        let map = parse_diskstats_package_sizes(text).unwrap();
        assert_eq!(
            map["c.d"],
            AppStorage {
                app_bytes: Some(20),
                data_bytes: None,
                cache_bytes: Some(2),
            }
        );
    }

    #[test]
    fn a_negative_size_is_unknown_not_zero() {
        let text = "Package Names: [\"a.b\"]\nApp Sizes: [-1]\nCache Sizes: [0]\n";
        let map = parse_diskstats_package_sizes(text).unwrap();
        assert_eq!(map["a.b"].app_bytes, None);
        assert_eq!(map["a.b"].data_bytes, None);
        assert_eq!(map["a.b"].cache_bytes, Some(0));
    }

    #[test]
    fn pm_path_and_stat_sum_a_split_apk() {
        let paths = parse_pm_path_apks(PM_PATH).expect("two apks");
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("/base.apk"));
        assert_eq!(parse_stat_sizes_total(STAT, paths.len()), Some(57_071_676));
    }

    #[test]
    fn pm_path_refuses_shell_metacharacters() {
        assert_eq!(parse_pm_path_apks("package:/data/app/x;rm -rf /\n"), None);
        assert_eq!(parse_pm_path_apks("package:relative.apk\n"), None);
        assert_eq!(parse_pm_path_apks(""), None);
    }

    #[test]
    fn stat_short_of_one_size_per_file_is_unavailable() {
        assert_eq!(parse_stat_sizes_total("8262729\n", 2), None);
        assert_eq!(
            parse_stat_sizes_total("8262729\nstat: 'x': No such file or directory\n", 2),
            None
        );
        assert_eq!(parse_stat_sizes_total("", 0), None);
    }
}
