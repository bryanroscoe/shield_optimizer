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
    ActionMethod, AppEntry, OptimizeAction, OptimizeMode, OptimizePlanItem, SkipReason,
};

/// What the host knows about the device right now — fed into `compute_plan`.
#[derive(Debug, Clone)]
pub struct OptimizeInputs<'a> {
    pub installed_packages: &'a HashSet<String>,
    pub disabled_packages: &'a HashSet<String>,
    /// Package → MB currently in use. Missing key = not running.
    pub memory_map: &'a HashMap<String, f64>,
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
/// - Otherwise → `Disable` or `Uninstall` per entry.method.
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
            let action = decide(entry, mode, inputs);
            let memory_mb = inputs.memory_map.get(&entry.package).copied();
            OptimizePlanItem {
                entry: entry.clone(),
                action,
                memory_mb,
            }
        })
        .collect();
    OptimizePlan { mode, items }
}

fn decide(entry: &AppEntry, mode: OptimizeMode, inputs: &OptimizeInputs) -> OptimizeAction {
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
            match entry.method {
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
            },
        );
        assert_eq!(plan.items[0].memory_mb, Some(153.4));
    }

    #[test]
    fn uninstall_method_chosen_when_optimize_and_installed() {
        let apps = vec![app("com.a", ActionMethod::Uninstall)];
        let installed = set(&["com.a"]);
        let disabled = set(&[]);
        let memory = HashMap::new();
        let plan = compute_plan(
            &apps,
            OptimizeMode::Optimize,
            &OptimizeInputs {
                installed_packages: &installed,
                disabled_packages: &disabled,
                memory_map: &memory,
            },
        );
        assert_eq!(plan.items[0].action, OptimizeAction::Uninstall);
    }
}
