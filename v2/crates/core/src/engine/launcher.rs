//! Custom launcher catalog + plan helpers.
//!
//! The catalog itself is data: `crates/core/data/app-lists/launchers.json`,
//! parsed by the loader and handed to these pure functions as an argument.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One supported launcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherEntry {
    pub name: String,
    pub package: String,
    /// The launcher's official source page, also useful when already installed.
    /// `None` for stock launchers, which ship with the device, and for
    /// HOME handlers discovered on the device rather than read from the file.
    #[serde(default)]
    pub source_url: Option<String>,
    /// Transient HOME holders (see `transient_home_holders`) that a takeover
    /// disables together with this stock launcher, because with stock gone
    /// they take the Home button back (#122). Stock entries only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disable_with: Vec<String>,
}

/// The launcher catalog, as loaded from `launchers.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LauncherCatalog {
    /// Installable third-party launchers — listed even when missing, so there
    /// is always a path to install one.
    pub custom: Vec<LauncherEntry>,
    /// Preinstalled launchers. Disabled when a custom launcher takes over, and
    /// listed (when present) so "back to stock" is one click away.
    pub stock: Vec<LauncherEntry>,
    /// Friendly names for known HOME-capable apps that aren't launchers.
    #[serde(default)]
    pub home_handler_names: BTreeMap<String, String>,
    /// HOME handlers that hold Home only in passing, while Android settles on
    /// a new default (Google TV's Setup Wraith, #122). Seeing one while
    /// verifying a switch means "not decided yet", never "the switch failed".
    #[serde(default)]
    pub transient_home_holders: Vec<String>,
}

impl LauncherCatalog {
    /// Friendly name for a known HOME-capable app that isn't a launcher.
    pub fn home_handler_name(&self, pkg: &str) -> Option<&str> {
        self.home_handler_names.get(pkg).map(String::as_str)
    }

    /// True when `pkg` is a preinstalled launcher we know by name.
    pub fn is_stock(&self, pkg: &str) -> bool {
        self.stock.iter().any(|e| e.package == pkg)
    }

    /// True when `pkg` only ever holds Home in passing.
    pub fn is_transient_home_holder(&self, pkg: &str) -> bool {
        self.transient_home_holders.iter().any(|p| p == pkg)
    }

    /// Transient holders to disable together with `stock_pkg`. Empty for a
    /// package that isn't a stock launcher or has no pairing.
    pub fn disable_with_for(&self, stock_pkg: &str) -> &[String] {
        self.stock
            .iter()
            .find(|e| e.package == stock_pkg)
            .map(|e| e.disable_with.as_slice())
            .unwrap_or(&[])
    }

    /// True when `pkg` appears in either catalog list.
    pub fn contains(&self, pkg: &str) -> bool {
        self.is_stock(pkg) || self.custom.iter().any(|e| e.package == pkg)
    }
}

/// One row of the Launchers list: a catalog entry plus its on-device state.
#[derive(Debug, Clone, Serialize)]
pub struct LauncherStatus {
    pub entry: LauncherEntry,
    pub installed: bool,
    pub enabled: bool,
    /// True for the device's preinstalled launcher(s) — rendered with a STOCK
    /// badge and no Install button, since they aren't on the Play Store.
    pub stock: bool,
    /// True for HOME-capable apps outside both catalogs (e.g. Setup Wraith,
    /// a sideloaded HOME app) — rendered with a HOME APP badge.
    pub other: bool,
    /// A catalog transient HOME holder (Google TV's Setup Wraith): it declares
    /// HOME but is the setup wizard, not a launcher. Never offered as the
    /// default, and disabling it is behind a confirm that explains the risk.
    #[serde(default)]
    pub setup_helper: bool,
}

/// Build the Launchers list: stock launchers actually present on the device
/// first (so "back to stock" is always one click away), then the full custom
/// catalog, then any other HOME-capable app. Custom launchers are listed even
/// when missing (they get an Install button); stock ones only when installed —
/// a Shield shouldn't show a "missing" Amazon launcher row.
///
/// `home_handler_pkgs` is the device's enabled HOME handlers (disabled
/// packages don't answer the HOME intent query, which is why callers pass
/// `tracked_disabled_pkgs` — handlers we disabled ourselves and remembered).
/// Safe fallbacks (Settings) are deliberately absent: they must never be
/// disabled, so we don't render them at all.
pub fn launcher_rows(
    catalog: &LauncherCatalog,
    installed_pkgs: &[String],
    disabled_pkgs: &[String],
    home_handler_pkgs: &[String],
    tracked_disabled_pkgs: &[String],
) -> Vec<LauncherStatus> {
    let is_disabled = |pkg: &str| disabled_pkgs.iter().any(|d| d == pkg);
    let stock_catalog = &catalog.stock;
    let custom_catalog = &catalog.custom;

    let stock = stock_catalog
        .iter()
        .filter(|e| installed_pkgs.iter().any(|p| p == &e.package))
        .map(|entry| LauncherStatus {
            enabled: !is_disabled(&entry.package),
            installed: true,
            stock: true,
            other: false,
            setup_helper: false,
            entry: entry.clone(),
        });

    let custom = custom_catalog.iter().map(|entry| {
        let installed = installed_pkgs.iter().any(|p| p == &entry.package);
        LauncherStatus {
            installed,
            enabled: installed && !is_disabled(&entry.package),
            stock: false,
            other: false,
            setup_helper: false,
            entry: entry.clone(),
        }
    });

    let mut seen_other = std::collections::HashSet::new();
    // A disabled transient holder no longer answers the HOME query, and a
    // takeover disables it without tracking it, so it is read from the package
    // state instead: it keeps its row (and its re-enable path) while installed.
    let transient_installed: Vec<String> = catalog
        .transient_home_holders
        .iter()
        .filter(|pkg| installed_pkgs.iter().any(|p| p == *pkg))
        // Only on Google TV, where a paired stock launcher is installed: a
        // Shield ships the package disabled and has no use for the row.
        .filter(|pkg| {
            catalog.stock.iter().any(|e| {
                e.disable_with.iter().any(|h| h == *pkg)
                    && installed_pkgs.iter().any(|p| p == &e.package)
            })
        })
        .cloned()
        .collect();
    let other = home_handler_pkgs
        .iter()
        .chain(tracked_disabled_pkgs.iter())
        .chain(transient_installed.iter())
        .filter(|pkg| {
            !catalog.contains(pkg)
                && !safe_home_handlers().contains(&pkg.as_str())
                && seen_other.insert(pkg.to_string())
        })
        .map(|pkg| LauncherStatus {
            entry: LauncherEntry {
                name: catalog.home_handler_name(pkg).unwrap_or(pkg).to_string(),
                package: pkg.clone(),
                source_url: None,
                disable_with: Vec::new(),
            },
            installed: true,
            enabled: !is_disabled(pkg),
            stock: false,
            other: true,
            setup_helper: catalog.is_transient_home_holder(pkg),
        });

    stock.chain(custom).chain(other).collect()
}

/// The transient HOME holders to disable together with `stocks`, deduplicated
/// and limited to the ones the catalog knows as transient and `enabled` lists.
pub fn paired_transient_holders(
    catalog: &LauncherCatalog,
    stocks: &[String],
    enabled: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for stock in stocks {
        for holder in catalog.disable_with_for(stock) {
            if catalog.is_transient_home_holder(holder)
                && enabled.iter().any(|e| e == holder)
                && !out.contains(holder)
            {
                out.push(holder.clone());
            }
        }
    }
    out
}

/// True when disabling `target` would leave the device without a single
/// enabled HOME handler the user can actually land on. Safe fallbacks
/// (Settings) don't count — they're a recovery hatch, not a launcher — and
/// neither do transient holders (Setup Wraith): with only it left, Home lands
/// on the setup wizard or a black screen, not a home screen.
pub fn is_last_enabled_home_handler(
    target: &str,
    enabled_handler_pkgs: &[String],
    catalog: &LauncherCatalog,
) -> bool {
    let target_is_enabled = enabled_handler_pkgs.iter().any(|h| h == target);
    let remaining = enabled_handler_pkgs
        .iter()
        .filter(|h| {
            h.as_str() != target
                && !safe_home_handlers().contains(&h.as_str())
                && !catalog.is_transient_home_holder(h)
        })
        .count();
    target_is_enabled && remaining == 0
}

/// Why a bulk disable (a snapshot restore) leaves a Home app on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeKeepReason {
    /// Disabling it would leave no enabled launcher.
    OnlyHomeApp,
    /// The Home-app list couldn't be read, so no disable of a Home app can be
    /// proven to leave one.
    HomeAppsUnreadable,
    /// The stock launcher it is paired with stays on, and the two go off
    /// together or not at all (#157, #158).
    PairedStockKept,
}

impl HomeKeepReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::OnlyHomeApp => "only Home app",
            Self::HomeAppsUnreadable => "Home apps unreadable",
            Self::PairedStockKept => "its stock launcher stays on",
        }
    }
}

/// Whether `pkg` may be a Home app. Without a readable handler list only the
/// catalog's launchers and setup helpers and the current Home count, so an
/// unreadable list never blocks unrelated apps.
pub fn may_be_home_handler(
    pkg: &str,
    enabled_handlers: Option<&[String]>,
    current_home: Option<&str>,
    catalog: &LauncherCatalog,
) -> bool {
    catalog.contains(pkg)
        || catalog.is_transient_home_holder(pkg)
        || current_home == Some(pkg)
        || enabled_handlers.is_some_and(|h| h.iter().any(|x| x == pkg))
}

/// Why disabling `pkg` must be refused so the TV keeps a Home screen, or
/// `None` when it may go. `enabled_handlers` is what the HOME query listed;
/// `None` or an empty list means it couldn't be read, which refuses every
/// Home candidate. `kept` names the packages this run already left on.
pub fn home_keep_reason(
    pkg: &str,
    enabled_handlers: Option<&[String]>,
    current_home: Option<&str>,
    kept: &[String],
    catalog: &LauncherCatalog,
) -> Option<HomeKeepReason> {
    if !may_be_home_handler(pkg, enabled_handlers, current_home, catalog) {
        return None;
    }
    let paired_stock_kept = catalog.is_transient_home_holder(pkg)
        && catalog.stock.iter().any(|stock| {
            stock.disable_with.iter().any(|h| h == pkg) && kept.contains(&stock.package)
        });
    if paired_stock_kept {
        return Some(HomeKeepReason::PairedStockKept);
    }
    match enabled_handlers {
        Some(handlers) if !handlers.is_empty() => {
            is_last_enabled_home_handler(pkg, handlers, catalog)
                .then_some(HomeKeepReason::OnlyHomeApp)
        }
        _ => Some(HomeKeepReason::HomeAppsUnreadable),
    }
}

/// The order a bulk disable runs in: unrelated apps, then other Home apps,
/// then stock launchers, then setup helpers after the stock launcher they
/// follow. Stock goes last among launchers so that when only one Home app can
/// stay, it is the one the TV shipped with. Stable within each group.
pub fn order_for_home_safety(
    pkgs: &[String],
    enabled_handlers: Option<&[String]>,
    current_home: Option<&str>,
    catalog: &LauncherCatalog,
) -> Vec<String> {
    let rank = |p: &str| {
        if catalog.is_transient_home_holder(p) {
            3
        } else if catalog.is_stock(p) {
            2
        } else if may_be_home_handler(p, enabled_handlers, current_home, catalog) {
            1
        } else {
            0
        }
    };
    let mut out = pkgs.to_vec();
    out.sort_by_key(|p| rank(p));
    out
}

/// Split a bulk disable into the packages to run, in run order, and the Home
/// apps to keep on. Each allowed disable leaves the handler list before the
/// next decision, as it would on the device.
pub fn plan_home_safe_disables(
    pkgs: &[String],
    enabled_handlers: Option<&[String]>,
    current_home: Option<&str>,
    catalog: &LauncherCatalog,
) -> (Vec<String>, Vec<(String, HomeKeepReason)>) {
    let mut remaining = enabled_handlers
        .filter(|h| !h.is_empty())
        .map(<[String]>::to_vec);
    let mut run = Vec::new();
    let mut kept: Vec<(String, HomeKeepReason)> = Vec::new();
    for pkg in order_for_home_safety(pkgs, enabled_handlers, current_home, catalog) {
        let kept_names: Vec<String> = kept.iter().map(|(p, _)| p.clone()).collect();
        match home_keep_reason(
            &pkg,
            remaining.as_deref(),
            current_home,
            &kept_names,
            catalog,
        ) {
            Some(reason) => kept.push((pkg, reason)),
            None => {
                if let Some(handlers) = remaining.as_mut() {
                    handlers.retain(|h| h != &pkg);
                }
                run.push(pkg);
            }
        }
    }
    (run, kept)
}

/// Which package holds Home, and how sure the reading is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct HomeReading {
    pub package: Option<String>,
    /// The activity, only when the resolver named the same package.
    pub activity: Option<String>,
    /// Set when the role holder and the resolver disagreed, for the
    /// diagnostics transcript. Never shown as a headline.
    pub note: Option<String>,
}

/// Decide the current Home app from the two things Android will tell us.
///
/// `role_holders` is `cmd role get-role-holders android.app.role.HOME`
/// (`None` when the build has no such command or it failed); `resolved` is
/// the `pkg/activity` component `resolve-activity` named.
///
/// On Android 10+ the HOME role holder is what the Home key opens. The
/// resolver ranks intent filters, so a high-priority HOME filter such as
/// Setup Wraith's can win it while the role, and the Home key, belong to the
/// launcher the user chose (#122). The role holder is therefore the answer
/// whenever there is one; the resolver is the fallback for builds without
/// the role command.
///
/// One exception: an enabled stock launcher the resolver names stays the
/// answer. Some builds accept a role change and keep opening stock on Home
/// (Shield / Android 11); only the resolver shows that.
pub fn pick_current_home(
    role_holders: Option<&[String]>,
    resolved: Option<&str>,
    catalog: &LauncherCatalog,
) -> HomeReading {
    let resolved = resolved.and_then(|c| c.split_once('/'));
    let role = role_holders.and_then(|r| r.iter().find(|p| is_valid_package_name(p)));
    match (role, resolved) {
        (None, Some((pkg, activity))) => HomeReading {
            package: Some(pkg.to_string()),
            activity: Some(activity.to_string()),
            note: None,
        },
        (None, None) => HomeReading::default(),
        (Some(role), Some((pkg, activity))) if pkg == role => HomeReading {
            package: Some(role.clone()),
            activity: Some(activity.to_string()),
            note: None,
        },
        (Some(role), None) => HomeReading {
            package: Some(role.clone()),
            activity: None,
            note: None,
        },
        (Some(role), Some((pkg, activity))) if catalog.is_stock(pkg) => HomeReading {
            package: Some(pkg.to_string()),
            activity: Some(activity.to_string()),
            note: Some(format!(
                "the HOME role is held by {role} but resolve-activity HOME names the stock \
                 launcher {pkg}, which overrides the role on this build; showing {pkg}"
            )),
        },
        (Some(role), Some((pkg, _))) => HomeReading {
            package: Some(role.clone()),
            activity: None,
            note: Some(if catalog.is_transient_home_holder(pkg) {
                format!(
                    "resolve-activity HOME named {pkg}, a setup helper with a high-priority \
                     HOME filter; the HOME role holder {role} is what the Home key opens"
                )
            } else {
                format!(
                    "resolve-activity HOME named {pkg} but the HOME role is held by {role}; \
                     showing the role holder"
                )
            }),
        },
    }
}

/// HOME-capable packages that we must NEVER disable — fallback safety net.
pub fn safe_home_handlers() -> &'static [&'static str] {
    &["com.android.tv.settings", "com.android.settings"]
}

/// Validate a user-supplied custom launcher package name (Setup-Launcher's
/// "Custom..." option in v1). Matches the regex v1 uses.
pub fn is_valid_package_name(pkg: &str) -> bool {
    if pkg.is_empty() {
        return false;
    }
    // Must look like `name(.name)+` where each segment starts with a letter.
    let segments: Vec<&str> = pkg.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    segments.iter().all(|seg| {
        !seg.is_empty()
            && seg.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A stand-in for the shipped `launchers.json`. The engine is handed a
    /// catalog rather than owning one, so these tests exercise the row logic
    /// against a fixture; the shipped file's own contents (Monet, Dispatch's
    /// package, the entry count) are asserted in the loader's tests, against
    /// the real JSON.
    fn catalog() -> LauncherCatalog {
        let entry = |name: &str, package: &str, source_url: Option<&str>| LauncherEntry {
            name: name.to_string(),
            package: package.to_string(),
            source_url: source_url.map(str::to_string),
            disable_with: Vec::new(),
        };
        LauncherCatalog {
            custom: vec![
                entry(
                    "Projectivy Launcher",
                    "com.spocky.projengmenu",
                    Some("https://example.invalid/projectivy"),
                ),
                entry("FLauncher", "me.efesser.flauncher", None),
            ],
            stock: vec![
                entry(
                    "Android TV Launcher (Stock)",
                    "com.google.android.tvlauncher",
                    None,
                ),
                entry("Amazon TV Launcher (Stock)", "com.amazon.tv.launcher", None),
            ],
            home_handler_names: [(
                "com.google.android.tungsten.setupwraith".to_string(),
                "Setup Wraith (HOME)".to_string(),
            )]
            .into_iter()
            .collect(),
            transient_home_holders: vec!["com.google.android.tungsten.setupwraith".to_string()],
        }
    }

    fn pkgs(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn launcher_rows_put_installed_stock_first() {
        let cat = catalog();
        let rows = launcher_rows(
            &cat,
            &pkgs(&["com.google.android.tvlauncher", "com.spocky.projengmenu"]),
            &[],
            &[],
            &[],
        );
        assert_eq!(rows[0].entry.package, "com.google.android.tvlauncher");
        assert!(rows[0].stock);
        assert!(rows[0].installed);
        assert!(rows[0].enabled);
        // The installed stock launcher + every custom catalog entry.
        assert_eq!(rows.len(), 1 + cat.custom.len());
        assert!(rows[1..].iter().all(|r| !r.stock));
    }

    #[test]
    fn launcher_rows_omit_stock_not_on_device() {
        // A Shield shouldn't get an Amazon (or any "missing stock") row.
        let cat = catalog();
        let rows = launcher_rows(&cat, &pkgs(&["com.spocky.projengmenu"]), &[], &[], &[]);
        assert!(rows.iter().all(|r| !r.stock));
        assert_eq!(rows.len(), cat.custom.len());
    }

    #[test]
    fn launcher_rows_carry_the_catalog_source_url() {
        // The "Get" link on a missing launcher is the only way to install one
        // that isn't on the device's Play Store.
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&["com.google.android.tvlauncher"]),
            &[],
            &[],
            &[],
        );
        let projectivy = rows
            .iter()
            .find(|r| r.entry.package == "com.spocky.projengmenu")
            .expect("custom row");
        assert!(!projectivy.installed);
        assert_eq!(
            projectivy.entry.source_url.as_deref(),
            Some("https://example.invalid/projectivy")
        );
    }

    #[test]
    fn launcher_rows_mark_disabled_stock() {
        // The post-cleanup state: stock disabled, custom active. The stock
        // row must still appear — it's the path back.
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&["com.google.android.tvlauncher", "com.spocky.projengmenu"]),
            &pkgs(&["com.google.android.tvlauncher"]),
            &[],
            &[],
        );
        assert!(rows[0].stock);
        assert!(rows[0].installed);
        assert!(!rows[0].enabled);
    }

    #[test]
    fn launcher_rows_include_other_home_handlers_but_never_safe_fallbacks() {
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&["com.spocky.projengmenu"]),
            &[],
            &pkgs(&[
                "com.spocky.projengmenu",                  // catalog — already a row
                "com.android.tv.settings",                 // safe fallback — never shown
                "com.google.android.tungsten.setupwraith", // genuinely "other"
            ]),
            &[],
        );
        let others: Vec<_> = rows.iter().filter(|r| r.other).collect();
        assert_eq!(others.len(), 1);
        assert_eq!(
            others[0].entry.package,
            "com.google.android.tungsten.setupwraith"
        );
        assert_eq!(others[0].entry.name, "Setup Wraith (HOME)");
        assert!(others[0].enabled);
        assert!(!rows
            .iter()
            .any(|r| r.entry.package == "com.android.tv.settings"));
    }

    #[test]
    fn launcher_rows_keep_tracked_disabled_handlers_visible() {
        // A disabled handler doesn't answer the HOME query — the tracked list
        // is what keeps its row (and its Enable path) alive.
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&["com.spocky.projengmenu"]),
            &pkgs(&["com.example.sideloaded.home"]),
            &[],
            &pkgs(&["com.example.sideloaded.home"]),
        );
        let row = rows
            .iter()
            .find(|r| r.entry.package == "com.example.sideloaded.home")
            .unwrap();
        assert!(row.other);
        assert!(!row.enabled);
    }

    #[test]
    fn last_enabled_home_handler_guard() {
        let enabled = pkgs(&["com.spocky.projengmenu", "com.android.tv.settings"]);
        // Projectivy is the only real launcher left — Settings doesn't count.
        assert!(is_last_enabled_home_handler(
            "com.spocky.projengmenu",
            &enabled,
            &catalog()
        ));

        let two = pkgs(&["com.spocky.projengmenu", "com.google.android.tvlauncher"]);
        assert!(!is_last_enabled_home_handler(
            "com.spocky.projengmenu",
            &two,
            &catalog()
        ));

        // Target already disabled (absent from the enabled list) — nothing to guard.
        assert!(!is_last_enabled_home_handler(
            "com.example.gone",
            &pkgs(&["com.spocky.projengmenu"]),
            &catalog()
        ));

        // Setup Wraith left alone is no home screen (#122).
        let with_wraith = pkgs(&["com.spocky.projengmenu", WRAITH]);
        assert!(is_last_enabled_home_handler(
            "com.spocky.projengmenu",
            &with_wraith,
            &catalog()
        ));
        // Disabling Wraith itself never takes the last launcher.
        assert!(!is_last_enabled_home_handler(
            WRAITH,
            &with_wraith,
            &catalog()
        ));
    }

    const WRAITH: &str = "com.google.android.tungsten.setupwraith";
    const MONET: &str = "com.klevico.monet";

    #[test]
    fn current_home_prefers_the_role_holder_over_a_transient_resolver_answer() {
        // The #122 state: stock disabled, Setup Wraith wins the resolver on
        // filter priority, Monet holds the HOME role and the Home key.
        let reading = pick_current_home(
            Some(&pkgs(&[MONET])),
            Some(&format!("{WRAITH}/.ui.MainActivity")),
            &catalog(),
        );
        assert_eq!(reading.package.as_deref(), Some(MONET));
        assert_eq!(reading.activity, None);
        assert!(reading.note.unwrap().contains("setup helper"));
    }

    #[test]
    fn current_home_agreeing_sources_keep_the_activity() {
        let reading = pick_current_home(
            Some(&pkgs(&[MONET])),
            Some(&format!("{MONET}/.MainActivity")),
            &catalog(),
        );
        assert_eq!(
            reading,
            HomeReading {
                package: Some(MONET.to_string()),
                activity: Some(".MainActivity".to_string()),
                note: None,
            }
        );
    }

    #[test]
    fn current_home_falls_back_to_the_resolver_without_a_role() {
        // Older SDK: no role command, or it printed nothing usable.
        for role in [
            None,
            Some(Vec::new()),
            Some(pkgs(&["Unknown command: get-role-holders"])),
        ] {
            let reading = pick_current_home(
                role.as_deref(),
                Some("com.google.android.tvlauncher/.MainActivity"),
                &catalog(),
            );
            assert_eq!(
                reading.package.as_deref(),
                Some("com.google.android.tvlauncher")
            );
            assert_eq!(reading.activity.as_deref(), Some(".MainActivity"));
            assert_eq!(reading.note, None);
        }
        assert_eq!(
            pick_current_home(None, None, &catalog()),
            HomeReading::default()
        );
    }

    #[test]
    fn current_home_keeps_a_stock_launcher_that_overrides_the_role() {
        // Accept-but-ignore builds: the role moved, Home still opens stock.
        let reading = pick_current_home(
            Some(&pkgs(&["com.spocky.projengmenu"])),
            Some("com.google.android.tvlauncher/.MainActivity"),
            &catalog(),
        );
        assert_eq!(
            reading.package.as_deref(),
            Some("com.google.android.tvlauncher")
        );
        assert_eq!(reading.activity.as_deref(), Some(".MainActivity"));
        assert!(reading.note.unwrap().contains("overrides the role"));
    }

    #[test]
    fn current_home_role_without_a_resolver_answer_is_the_role() {
        let reading = pick_current_home(Some(&pkgs(&[MONET])), None, &catalog());
        assert_eq!(reading.package.as_deref(), Some(MONET));
        assert_eq!(reading.note, None);
    }

    #[test]
    fn current_home_disagreement_shows_the_role_holder_with_a_note() {
        let reading = pick_current_home(
            Some(&pkgs(&[MONET])),
            Some("com.spocky.projengmenu/.ui.home.MainActivity"),
            &catalog(),
        );
        assert_eq!(reading.package.as_deref(), Some(MONET));
        let note = reading.note.unwrap();
        assert!(
            note.contains("com.spocky.projengmenu") && note.contains(MONET),
            "{note}"
        );
    }

    #[test]
    fn launcher_rows_mark_setup_wraith_as_a_setup_helper() {
        let rows = launcher_rows(&catalog(), &pkgs(&[MONET]), &[], &pkgs(&[WRAITH]), &[]);
        let row = rows.iter().find(|r| r.entry.package == WRAITH).unwrap();
        assert!(row.other && row.setup_helper);
        assert!(rows
            .iter()
            .filter(|r| r.entry.package != WRAITH)
            .all(|r| !r.setup_helper));
    }

    const GTV_HOME: &str = "com.google.android.apps.tv.launcherx";

    fn google_tv_catalog() -> LauncherCatalog {
        let mut cat = catalog();
        cat.stock.push(LauncherEntry {
            name: "Google TV Home (Stock)".to_string(),
            package: GTV_HOME.to_string(),
            source_url: None,
            disable_with: vec![WRAITH.to_string()],
        });
        cat
    }

    #[test]
    fn paired_holders_come_only_from_the_stock_being_disabled() {
        let cat = google_tv_catalog();
        let enabled = pkgs(&[MONET, GTV_HOME, WRAITH]);
        assert_eq!(
            paired_transient_holders(&cat, &pkgs(&[GTV_HOME]), &enabled),
            pkgs(&[WRAITH])
        );
        // Shield stock pairs with nothing, even with Setup Wraith enabled.
        assert!(paired_transient_holders(
            &cat,
            &pkgs(&["com.google.android.tvlauncher"]),
            &enabled
        )
        .is_empty());
        // Already disabled (absent from the enabled list): nothing to do.
        assert!(
            paired_transient_holders(&cat, &pkgs(&[GTV_HOME]), &pkgs(&[MONET, GTV_HOME]))
                .is_empty()
        );
        // Two stocks naming one holder disable it once.
        assert_eq!(
            paired_transient_holders(&cat, &pkgs(&[GTV_HOME, GTV_HOME]), &enabled),
            pkgs(&[WRAITH])
        );
    }

    #[test]
    fn a_pairing_must_name_a_catalogued_transient_holder() {
        let mut cat = google_tv_catalog();
        cat.stock.last_mut().unwrap().disable_with = pkgs(&["com.android.tv.settings"]);
        assert!(paired_transient_holders(
            &cat,
            &pkgs(&[GTV_HOME]),
            &pkgs(&["com.android.tv.settings"])
        )
        .is_empty());
    }

    #[test]
    fn wraith_alone_is_not_a_home_so_it_is_never_the_survivor() {
        // Stock gone, Wraith the only other handler: disabling Wraith would
        // leave nothing, and stock disabling would leave only Wraith.
        let cat = google_tv_catalog();
        assert!(is_last_enabled_home_handler(
            GTV_HOME,
            &pkgs(&[GTV_HOME, WRAITH]),
            &cat
        ));
        assert!(is_last_enabled_home_handler(WRAITH, &pkgs(&[WRAITH]), &cat));
    }

    #[test]
    fn a_bulk_disable_keeps_stock_and_its_helper_when_no_other_launcher_is_enabled() {
        // A snapshot from a TV whose Home was Projectivy, applied where
        // Projectivy isn't installed: stock is the only real Home app left.
        let cat = google_tv_catalog();
        let handlers = pkgs(&[GTV_HOME, WRAITH, "com.android.tv.settings"]);
        let (run, kept) = plan_home_safe_disables(
            &pkgs(&[WRAITH, GTV_HOME, "com.example.bloat"]),
            Some(&handlers),
            Some(GTV_HOME),
            &cat,
        );
        assert_eq!(run, pkgs(&["com.example.bloat"]));
        assert_eq!(
            kept,
            vec![
                (GTV_HOME.to_string(), HomeKeepReason::OnlyHomeApp),
                (WRAITH.to_string(), HomeKeepReason::PairedStockKept),
            ]
        );
    }

    #[test]
    fn a_bulk_disable_runs_stock_after_other_launchers_and_helpers_last() {
        let cat = google_tv_catalog();
        let handlers = pkgs(&[MONET, GTV_HOME, WRAITH, "com.spocky.projengmenu"]);
        let (run, kept) = plan_home_safe_disables(
            &pkgs(&[
                WRAITH,
                GTV_HOME,
                "com.spocky.projengmenu",
                "com.example.bloat",
            ]),
            Some(&handlers),
            Some(GTV_HOME),
            &cat,
        );
        assert_eq!(
            run,
            pkgs(&[
                "com.example.bloat",
                "com.spocky.projengmenu",
                GTV_HOME,
                WRAITH
            ])
        );
        assert!(kept.is_empty(), "{kept:?}");

        // Two launchers, one Home slot: stock is the one that stays.
        let handlers = pkgs(&[GTV_HOME, "com.spocky.projengmenu"]);
        let (run, kept) = plan_home_safe_disables(
            &pkgs(&[GTV_HOME, "com.spocky.projengmenu"]),
            Some(&handlers),
            None,
            &cat,
        );
        assert_eq!(run, pkgs(&["com.spocky.projengmenu"]));
        assert_eq!(
            kept,
            vec![(GTV_HOME.to_string(), HomeKeepReason::OnlyHomeApp)]
        );
    }

    #[test]
    fn an_unreadable_home_list_keeps_every_home_candidate_and_nothing_else() {
        let cat = google_tv_catalog();
        for unreadable in [None, Some(&[][..])] {
            let (run, kept) = plan_home_safe_disables(
                &pkgs(&[
                    "com.example.bloat",
                    GTV_HOME,
                    "com.spocky.projengmenu",
                    "com.example.homeapp",
                ]),
                unreadable,
                Some("com.example.homeapp"),
                &cat,
            );
            assert_eq!(run, pkgs(&["com.example.bloat"]));
            assert_eq!(
                kept.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
                ["com.spocky.projengmenu", "com.example.homeapp", GTV_HOME]
            );
            assert!(kept
                .iter()
                .all(|(_, r)| *r == HomeKeepReason::HomeAppsUnreadable));
        }
    }

    #[test]
    fn disabled_setup_wraith_keeps_its_row_without_being_tracked() {
        // A disabled package no longer answers the HOME query and a takeover
        // does not track it, so the package state alone has to surface it.
        let rows = launcher_rows(
            &google_tv_catalog(),
            &pkgs(&[MONET, GTV_HOME, WRAITH]),
            &pkgs(&[WRAITH]),
            &[],
            &[],
        );
        let row = rows.iter().find(|r| r.entry.package == WRAITH).unwrap();
        assert!(row.installed && !row.enabled && row.setup_helper && row.other);
        // Installed but disabled with no Google TV stock (a Shield): no row.
        let rows = launcher_rows(
            &google_tv_catalog(),
            &pkgs(&[MONET, WRAITH]),
            &pkgs(&[WRAITH]),
            &[],
            &[],
        );
        assert!(!rows.iter().any(|r| r.entry.package == WRAITH));
        // Not installed: no row at all.
        let rows = launcher_rows(&catalog(), &pkgs(&[MONET]), &[], &[], &[]);
        assert!(!rows.iter().any(|r| r.entry.package == WRAITH));
        // Enabled and answering the query: still one row.
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&[MONET, WRAITH]),
            &[],
            &pkgs(&[WRAITH]),
            &[],
        );
        assert_eq!(rows.iter().filter(|r| r.entry.package == WRAITH).count(), 1);
    }

    #[test]
    fn package_name_validation_accepts_valid() {
        for valid in &[
            "com.example.launcher",
            "tv.projectivy.launcher",
            "com.google.android.tvlauncher",
            "org.example.app123",
            "com.a.b",
            "com.Example_App.test",
        ] {
            assert!(is_valid_package_name(valid), "should accept {}", valid);
        }
    }

    #[test]
    fn package_name_validation_rejects_invalid() {
        for invalid in &[
            "",
            "   ",
            "com",
            "com.",
            ".com.example",
            "123.example.app",
            "com..example",
            "com.example.",
            "com.123.app",
            "-com.example.app",
            "com.example.app with spaces",
        ] {
            assert!(
                !is_valid_package_name(invalid),
                "should reject {:?}",
                invalid
            );
        }
    }
}
