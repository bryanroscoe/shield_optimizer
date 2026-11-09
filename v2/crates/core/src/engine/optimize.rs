//! Optimize / Restore planner — pure logic. v1 reference: `Run-Task` (§4.1 in
//! FEATURES.md). Given a device-specific app list plus a snapshot of the
//! device's current state (installed / disabled / memory map), produce one
//! `OptimizePlanItem` per app describing what the host should do.
//!
//! No ADB calls, no I/O. The host layer feeds this with parsed `pm list`
//! output and a memory map from `dumpsys meminfo`, then iterates the result
//! and dispatches the per-item commands.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::types::{
    ActionMethod, AppEntry, InstallSource, OptimizeAction, OptimizeMode, OptimizePlanItem,
    SkipReason,
};

/// What the host knows about the device right now — fed into `compute_plan`.
#[derive(Debug, Clone)]
pub struct OptimizeInputs<'a> {
    pub installed_packages: &'a HashSet<String>,
    pub disabled_packages: &'a HashSet<String>,
    /// Package → MB currently in use. Missing key = not running.
    pub memory_map: &'a HashMap<String, f64>,
    /// `pm list packages -3`: what this TV's user installed. `None` when that
    /// read failed, which makes every source `Unknown`.
    pub user_installed_packages: Option<&'a HashSet<String>>,
}

/// Output of `compute_plan`. One item per app entry in input order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizePlan {
    pub mode: OptimizeMode,
    pub items: Vec<OptimizePlanItem>,
}

/// Decide one action per app entry given the current device state.
///
/// Optimize mode:
/// - Not installed → `Skip(NotInstalled)`
/// - Already disabled and method is disable → `Skip(AlreadyDisabled)`
/// - Not installed (and method is uninstall) → `Skip(NotInstalled)` (covers the
///   "already uninstalled" branch since they're both absent from `pm list packages`)
/// - Otherwise → `Disable` or `Uninstall`: `safe_method` for default
///   Optimize rows, and for review rows the suggestion
///   (`AppEntry::suggested_method`), which uninstalls only an ordinary store
///   app the user installed.
///
/// Restore mode:
/// - Not installed → `Skip(NotInstalled)` (host can surface Play Store
///   separately; the engine doesn't decide that)
/// - Already enabled (not in disabled set) → `Skip(AlreadyEnabled)`
/// - Otherwise → `Enable`.
pub fn compute_plan(
    apps: &[AppEntry],
    mode: OptimizeMode,
    inputs: &OptimizeInputs,
) -> OptimizePlan {
    let items = apps
        .iter()
        .map(|entry| {
            let install_source = install_source(&entry.package, inputs);
            let action = decide(entry, mode, inputs, install_source);
            let memory_mb = inputs.memory_map.get(&entry.package).copied();
            OptimizePlanItem {
                entry: entry.clone(),
                action,
                memory_mb,
                install_source,
            }
        })
        .collect();
    OptimizePlan { mode, items }
}

/// How `pkg` got onto the TV. Only an installed package has a source, and
/// only a successful third-party read can say which.
pub fn install_source(pkg: &str, inputs: &OptimizeInputs) -> InstallSource {
    if !inputs.installed_packages.contains(pkg) {
        return InstallSource::Unknown;
    }
    match inputs.user_installed_packages {
        Some(user) if user.contains(pkg) => InstallSource::User,
        Some(_) => InstallSource::Preinstalled,
        None => InstallSource::Unknown,
    }
}

fn decide(
    entry: &AppEntry,
    mode: OptimizeMode,
    inputs: &OptimizeInputs,
    source: InstallSource,
) -> OptimizeAction {
    let installed = inputs.installed_packages.contains(&entry.package);
    let disabled = inputs.disabled_packages.contains(&entry.package);

    match mode {
        OptimizeMode::Optimize => {
            if !installed {
                return OptimizeAction::Skip {
                    reason: SkipReason::NotInstalled,
                };
            }
            if disabled {
                return OptimizeAction::Skip {
                    reason: SkipReason::AlreadyDisabled,
                };
            }
            // Gate uninstall to reinstallable apps — never recommend removing
            // something the user can't get back from the Play Store. A review
            // row follows the suggestion: Uninstall only for a store app the
            // user installed.
            let method = if entry.review && !entry.default_optimize {
                entry.suggested_method(source)
            } else {
                entry.safe_method()
            };
            match method {
                ActionMethod::Disable => OptimizeAction::Disable,
                ActionMethod::Uninstall => OptimizeAction::Uninstall,
            }
        }
        OptimizeMode::Restore => {
            if !installed {
                return OptimizeAction::Skip {
                    reason: SkipReason::NotInstalled,
                };
            }
            if !disabled {
                return OptimizeAction::Skip {
                    reason: SkipReason::AlreadyEnabled,
                };
            }
            OptimizeAction::Enable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::RiskTier;

    fn app(pkg: &str, method: ActionMethod) -> AppEntry {
        AppEntry {
            package: pkg.to_string(),
            name: pkg.to_string(),
            method,
            risk: RiskTier::Safe,
            optimize_description: String::new(),
            restore_description: String::new(),
            default_optimize: true,
            default_restore: true,
            play_store: false,
            defunct: false,
            review: false,
            reviewed_at: None,
            sources: Vec::new(),
            device_scope: Vec::new(),
        }
    }

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn optimize_skips_uninstalled_and_disabled() {
        let apps = vec![
            app("com.a", ActionMethod::Disable),
            app("com.b", ActionMethod::Uninstall),
            app("com.c", ActionMethod::Disable),
        ];
        let installed = set(&["com.a", "com.c"]);
        let disabled = set(&["com.c"]);
        let memory = HashMap::new();
        let plan = compute_plan(
            &apps,
            OptimizeMode::Optimize,
            &OptimizeInputs {
                installed_packages: &installed,
                disabled_packages: &disabled,
                memory_map: &memory,
                user_installed_packages: None,
            },
        );
        assert_eq!(plan.items[0].action, OptimizeAction::Disable);
        assert_eq!(
            plan.items[1].action,
            OptimizeAction::Skip {
                reason: SkipReason::NotInstalled
            }
        );
        assert_eq!(
            plan.items[2].action,
            OptimizeAction::Skip {
                reason: SkipReason::AlreadyDisabled
            }
        );
    }

    #[test]
    fn restore_skips_enabled_and_uninstalled() {
        let apps = vec![
            app("com.a", ActionMethod::Disable),
            app("com.b", ActionMethod::Disable),
            app("com.c", ActionMethod::Disable),
        ];
        let installed = set(&["com.a", "com.b"]);
        let disabled = set(&["com.b"]);
        let memory = HashMap::new();
        let plan = compute_plan(
            &apps,
            OptimizeMode::Restore,
            &OptimizeInputs {
                installed_packages: &installed,
                disabled_packages: &disabled,
                memory_map: &memory,
                user_installed_packages: None,
            },
        );
        assert_eq!(
            plan.items[0].action,
            OptimizeAction::Skip {
                reason: SkipReason::AlreadyEnabled
            }
        );
        assert_eq!(plan.items[1].action, OptimizeAction::Enable);
        assert_eq!(
            plan.items[2].action,
            OptimizeAction::Skip {
                reason: SkipReason::NotInstalled
            }
        );
    }

    #[test]
    fn memory_map_propagated() {
        let apps = vec![app("com.a", ActionMethod::Disable)];
        let installed = set(&["com.a"]);
        let disabled = set(&[]);
        let mut memory = HashMap::new();
        memory.insert("com.a".to_string(), 153.4);
        let plan = compute_plan(
            &apps,
            OptimizeMode::Optimize,
            &OptimizeInputs {
                installed_packages: &installed,
                disabled_packages: &disabled,
                memory_map: &memory,
                user_installed_packages: None,
            },
        );
        assert_eq!(plan.items[0].memory_mb, Some(153.4));
    }

    #[test]
    fn uninstall_method_chosen_when_optimize_and_installed() {
        // Reinstallable (on the Play Store) so uninstall is a safe recommendation.
        let mut a = app("com.a", ActionMethod::Uninstall);
        a.play_store = true;
        let installed = set(&["com.a"]);
        let disabled = set(&[]);
        let memory = HashMap::new();
        let plan = compute_plan(
            &[a],
            OptimizeMode::Optimize,
            &OptimizeInputs {
                installed_packages: &installed,
                disabled_packages: &disabled,
                memory_map: &memory,
                user_installed_packages: None,
            },
        );
        assert_eq!(plan.items[0].action, OptimizeAction::Uninstall);
    }

    #[test]
    fn uninstall_downgraded_to_disable_when_not_reinstallable() {
        // Active app, not on the Play Store and not defunct → uninstalling it is
        // a trap, so the planner must pick the reversible disable instead.
        let a = app("com.regional.app", ActionMethod::Uninstall); // play_store=false, defunct=false
        let installed = set(&["com.regional.app"]);
        let plan = compute_plan(
            &[a],
            OptimizeMode::Optimize,
            &OptimizeInputs {
                installed_packages: &installed,
                disabled_packages: &HashSet::new(),
                memory_map: &HashMap::new(),
                user_installed_packages: None,
            },
        );
        assert_eq!(plan.items[0].action, OptimizeAction::Disable);
    }

    fn review_store_app(pkg: &str, method: ActionMethod) -> AppEntry {
        let mut a = app(pkg, method);
        a.default_optimize = false;
        a.default_restore = false;
        a.review = true;
        a.play_store = true;
        a
    }

    #[test]
    fn suggestion_uninstalls_only_an_ordinary_store_app_the_user_installed() {
        for method in [ActionMethod::Disable, ActionMethod::Uninstall] {
            let a = review_store_app("com.store", method);
            assert_eq!(
                a.suggested_method(InstallSource::User),
                ActionMethod::Uninstall
            );
            for source in [InstallSource::Preinstalled, InstallSource::Unknown] {
                assert_eq!(
                    a.suggested_method(source),
                    ActionMethod::Disable,
                    "{method:?} {source:?}"
                );
            }
        }
    }

    #[test]
    fn suggestion_disables_anything_that_is_not_an_ordinary_store_app() {
        let mut sideload = review_store_app("com.sideload", ActionMethod::Uninstall);
        sideload.play_store = false;
        let mut defunct = review_store_app("com.defunct", ActionMethod::Uninstall);
        defunct.defunct = true;
        let mut restore = review_store_app("com.shipped", ActionMethod::Disable);
        restore.default_restore = true;
        let mut high = review_store_app("com.high", ActionMethod::Disable);
        high.risk = RiskTier::High;
        let mut advanced = review_store_app("com.advanced", ActionMethod::Disable);
        advanced.risk = RiskTier::Advanced;
        for a in [sideload, defunct, restore, high, advanced] {
            assert_eq!(
                a.suggested_method(InstallSource::User),
                ActionMethod::Disable,
                "{}",
                a.package
            );
        }
        let mut medium = review_store_app("com.medium", ActionMethod::Disable);
        medium.risk = RiskTier::Medium;
        assert_eq!(
            medium.suggested_method(InstallSource::User),
            ActionMethod::Uninstall
        );
    }

    #[test]
    fn review_rows_follow_the_install_source_and_default_rows_do_not() {
        let mine = review_store_app("com.mine", ActionMethod::Disable);
        let shipped = review_store_app("com.shipped", ActionMethod::Uninstall);
        let mut defunct_default = app("com.defunct", ActionMethod::Uninstall);
        defunct_default.defunct = true;
        defunct_default.default_restore = false;
        let installed = set(&["com.mine", "com.shipped", "com.defunct"]);
        let user = set(&["com.mine", "com.defunct"]);
        let plan = compute_plan(
            &[mine, shipped, defunct_default],
            OptimizeMode::Optimize,
            &OptimizeInputs {
                installed_packages: &installed,
                disabled_packages: &HashSet::new(),
                memory_map: &HashMap::new(),
                user_installed_packages: Some(&user),
            },
        );
        assert_eq!(plan.items[0].install_source, InstallSource::User);
        assert_eq!(plan.items[0].action, OptimizeAction::Uninstall);
        assert_eq!(plan.items[1].install_source, InstallSource::Preinstalled);
        assert_eq!(plan.items[1].action, OptimizeAction::Disable);
        // A default Optimize row keeps its curated, store-gated method.
        assert_eq!(plan.items[2].action, OptimizeAction::Uninstall);
    }

    #[test]
    fn a_missing_package_or_unread_source_is_unknown() {
        let installed = set(&["com.a"]);
        let user = set(&["com.a", "com.gone"]);
        let empty = HashMap::new();
        let disabled = HashSet::new();
        let read = OptimizeInputs {
            installed_packages: &installed,
            disabled_packages: &disabled,
            memory_map: &empty,
            user_installed_packages: Some(&user),
        };
        assert_eq!(install_source("com.a", &read), InstallSource::User);
        assert_eq!(install_source("com.gone", &read), InstallSource::Unknown);
        let unread = OptimizeInputs {
            user_installed_packages: None,
            ..read
        };
        assert_eq!(install_source("com.a", &unread), InstallSource::Unknown);
    }
}
