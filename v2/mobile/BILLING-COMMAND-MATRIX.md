# Current mobile command and UI entitlement matrix

This is current behavior, not a pricing/gate migration. Every registered/invoked command is listed; `BILLING-COMMAND-MATRIX.json` holds the same rows, and `tests/entitlement-preview.test.mjs` fails when a command is registered or removed without updating it. Keep this table in step with the JSON. Free means no Pro entitlement prerequisite; connection, permission, safety and path restrictions still apply. Signed-key activation remains validated, not an unrestricted unlock.

Callers name actual API call sites; `session.svelte.ts` mediates Onboarding/Devices connections and Dashboard/Diagnostics caches. Screens can show Pro badges without blocking navigation. An unused API wrapper is explicitly shown as having no direct caller.

| Command | Current backend gate | Command source (relative to mobile) | API callers |
|---|---|---|---|
| `wireless_discover` | Free | `src-tauri/src/wireless_commands.rs` | `src/lib/session.svelte.ts`, `src/screens/Onboarding.svelte` |
| `wireless_pair` | Free | `src-tauri/src/wireless_commands.rs` | `src/lib/session.svelte.ts` |
| `wireless_connect` | Free | `src-tauri/src/wireless_commands.rs` | `src/lib/session.svelte.ts` |
| `wireless_cancel_connect` | Free | `src-tauri/src/wireless_commands.rs` | `src/lib/session.svelte.ts` |
| `wireless_disconnect` | Free | `src-tauri/src/wireless_commands.rs` | `src/lib/session.svelte.ts` |
| `wireless_status` | Free | `src-tauri/src/wireless_commands.rs` | `src/lib/session.svelte.ts` |
| `find_remote` | Free | `src-tauri/src/wireless_commands.rs` | `src/components/FindRemoteButton.svelte` |
| `activate_license` | Free | `src-tauri/src/lib.rs` | `src/lib/session.svelte.ts` |
| `get_entitlement` | Free | `src-tauri/src/lib.rs` | `src/lib/session.svelte.ts` |
| `license_info` | Free | `../crates/core/src/commands/license.rs` | `src/screens/More.svelte` |
| `read_debug_log` | Free | `src-tauri/src/lib.rs` | `src/screens/More.svelte` |
| `list_devices` | Free | `../crates/core/src/commands/devices.rs` | `src/lib/session.svelte.ts` |
| `device_profile` | Free | `../crates/core/src/commands/devices.rs` | No direct API caller |
| `rename_device` | Free | `../crates/core/src/commands/devices.rs` | `src/screens/Devices.svelte` |
| `health_report` | Free | `../crates/core/src/commands/health.rs` | `src/lib/session.svelte.ts` |
| `media_report` | Free | `../crates/core/src/commands/health.rs` | `src/screens/Diagnostics.svelte` |
| `resource_sample` | Free | `../crates/core/src/commands/health.rs` | `src/screens/Diagnostics.svelte` |
| `app_list_for_device` | Free | `../crates/core/src/commands/health.rs` | `src/lib/session.svelte.ts`, `src/screens/Apps.svelte` |
| `report_all` | Free | `../crates/core/src/commands/health.rs` | No direct API caller |
| `list_launchers` | Free | `../crates/core/src/commands/launcher.rs` | `src/screens/Launcher.svelte` |
| `current_launcher` | Free | `../crates/core/src/commands/launcher.rs` | `src/screens/Launcher.svelte` |
| `channel_provider_disabled` | Free | `../crates/core/src/commands/launcher.rs` | `src/screens/Launcher.svelte` |
| `set_default_launcher` | LauncherTakeover | `../crates/core/src/commands/launcher.rs` | `src/screens/Launcher.svelte` |
| `disable_launcher` | CuratedDebloat | `../crates/core/src/commands/launcher.rs` | `src/screens/Launcher.svelte` |
| `set_home_any` | LauncherTakeover | `../crates/core/src/commands/launcher.rs` | `src/screens/Launcher.svelte` |
| `disable_stock_launcher` | LauncherTakeover | `../crates/core/src/commands/launcher.rs` | `src/screens/Launcher.svelte` |
| `disable_setup_helper` | LauncherTakeover | `../crates/core/src/commands/launcher.rs` | `src/screens/Launcher.svelte` |
| `force_stop` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte` |
| `take_screenshot` | Free | `../crates/core/src/commands/screenshot.rs` | `src/screens/Dashboard.svelte` |
| `package_states` | Free | `../crates/core/src/commands/apps.rs` | `src/lib/session.svelte.ts`, `src/screens/Apps.svelte`, `src/screens/Tweaks.svelte` |
| `list_other_packages` | Free | `../crates/core/src/commands/apps.rs` | `src/screens/Apps.svelte` |
| `list_installed_packages` | Free | `../crates/core/src/commands/apps.rs` | `src/screens/Backups.svelte`, `src/screens/Diagnostics.svelte`, `src/screens/Launcher.svelte` |
| `app_memory_map` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte` |
| `app_storage_map` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte` |
| `app_apk_size` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppDetailSheet.svelte` |
| `app_usage_map` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte` |
| `safety_info` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte`, `src/screens/Diagnostics.svelte`, `src/screens/Optimize.svelte` |
| `process_safety_info` | Free | `../crates/core/src/commands/apps.rs` | `src/screens/Diagnostics.svelte` |
| `trim_caches` | Free | `../crates/core/src/commands/apps.rs` | `src/screens/Dashboard.svelte` |
| `disable_package` | CuratedDebloat | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte`, `src/screens/Optimize.svelte`, `src/screens/Tweaks.svelte` |
| `enable_package` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte`, `src/screens/Launcher.svelte`, `src/screens/Optimize.svelte`, `src/screens/Tweaks.svelte` |
| `uninstall_package` | CuratedDebloat | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte`, `src/screens/Optimize.svelte` |
| `reinstall_existing` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte` |
| `set_app_permission` | AppPermissionWrite | `../crates/core/src/commands/apps.rs` | `src/screens/Tweaks.svelte` |
| `app_permission_state` | Free | `../crates/core/src/commands/apps.rs` | `src/screens/Tweaks.svelte` |
| `set_app_op` | AppPermissionWrite | `../crates/core/src/commands/apps.rs` | No direct API caller |
| `get_app_op` | Free | `../crates/core/src/commands/apps.rs` | No direct API caller |
| `open_play_store` | Free | `../crates/core/src/commands/apps.rs` | `src/components/AppSheetHost.svelte`, `src/screens/Apps.svelte`, `src/screens/Launcher.svelte` |
| `send_text` | Free | `../crates/core/src/commands/input.rs` | `src/screens/Remote.svelte` |
| `send_key` | Free | `../crates/core/src/commands/input.rs` | `src/screens/Remote.svelte` |
| `open_settings` | Free | `../crates/core/src/commands/input.rs` | `src/screens/Remote.svelte` |
| `remote_warm` | Free | `../crates/core/src/commands/input.rs` | `src/screens/Remote.svelte` |
| `panic_recovery` | Free | `../crates/core/src/commands/recovery.rs` | `src/screens/More.svelte` |
| `reboot_device` | conditional: normal Free; recovery/bootloader AdvancedReboot | `../crates/core/src/commands/reboot.rs` | `src/screens/Dashboard.svelte`, `src/screens/More.svelte` |
| `prepare_optimize` | OptimizeWizard | `../crates/core/src/commands/optimize.rs` | `src/screens/Optimize.svelte` |
| `apply_performance_settings` | TweaksWrite | `../crates/core/src/commands/optimize.rs` | `src/screens/Optimize.svelte` |
| `get_tweaks` | Free | `../crates/core/src/commands/tuning.rs` | `src/screens/Tweaks.svelte` |
| `write_setting` | TweaksWrite | `../crates/core/src/commands/tuning.rs` | `src/screens/Tweaks.svelte` |
| `get_display_scaling` | Free | `../crates/core/src/commands/tuning.rs` | `src/screens/Tweaks.svelte` |
| `set_display_scaling` | TweaksWrite | `../crates/core/src/commands/tuning.rs` | `src/screens/Tweaks.svelte` |
| `get_private_dns` | Free | `../crates/core/src/commands/tuning.rs` | `src/screens/Tweaks.svelte` |
| `set_private_dns` | TweaksWrite | `../crates/core/src/commands/tuning.rs` | `src/screens/Tweaks.svelte` |
| `delete_snapshot` | Snapshot | `../crates/core/src/commands/snapshot.rs` | `src/screens/Snapshots.svelte` |
| `snapshot_dir_path` | Snapshot | `../crates/core/src/commands/snapshot.rs` | No direct API caller |
| `list_snapshots` | Snapshot | `../crates/core/src/commands/snapshot.rs` | `src/screens/Snapshots.svelte` |
| `save_snapshot` | Snapshot | `../crates/core/src/commands/snapshot.rs` | `src/screens/Launcher.svelte`, `src/screens/Snapshots.svelte` |
| `apply_snapshot` | Snapshot | `../crates/core/src/commands/snapshot.rs` | `src/screens/Snapshots.svelte` |
| `preview_apply` | Snapshot | `../crates/core/src/commands/snapshot.rs` | `src/screens/Snapshots.svelte` |
| `list_remote_dir` | Free | `src-tauri/src/file_commands.rs` | `src/screens/Files.svelte` |
| `pull_file` | Free | `src-tauri/src/file_commands.rs` | `src/screens/Files.svelte` |
| `backup_apk` | Free | `src-tauri/src/file_commands.rs` | `src/screens/Backups.svelte` |
| `restore_apk_backup` | Free | `src-tauri/src/file_commands.rs` | `src/screens/Backups.svelte` |
| `list_backups` | Free | `src-tauri/src/file_commands.rs` | `src/screens/Backups.svelte` |
| `delete_backup` | Free | `src-tauri/src/file_commands.rs` | `src/screens/Backups.svelte` |
| `find_files` | Free | `src-tauri/src/file_commands.rs` | `src/screens/Files.svelte` |
| `delete_path` | Free | `src-tauri/src/file_commands.rs` | `src/screens/Files.svelte` |
| `plugin:opener|open_url` | Free | `src/lib/api.ts (scoped opener plugin)` | `src/components/AppReportSheet.svelte`, `src/screens/Launcher.svelte` |

## UI and recovery findings

- Onboarding/Devices and compatibility reads are Free. Read errors/unknown evidence must stay honest.
- More marks Optimize, Launcher and Snapshots as Pro tools; navigation is still allowed, with per-action LOCKED handling. Tweaks current-value reads are Free; setters are Pro.
- Apps enable/reinstall and More emergency recovery are Free; the latter re-enables disabled packages rather than undoing every change. Existing UI/backend behavior is tested without widening gates.
- Optimize restore still requires Pro to prepare the plan; snapshots list/delete/apply are Pro-gated. Snapshot apply can disable packages and change settings, so it cannot be made Free as arbitrary undo.
- Settings, permission and Home before-value restore remain gated or incomplete. A bounded journal/undo mechanism needs separate review. This slice creates no persistent journal and does not make arbitrary writes free.
- FileManager/BackupClone enum labels do not imply enforcement: mobile file/backup handlers currently have no Pro prerequisite. No gate changes are proposed here.
- Remote basic input, file paths, package names, safety checks and connection-generation fencing remain independent safety boundaries.
- The new entitlement preview is isolated and unused by session/backend authorization; store purchase callbacks alone cannot grant real Pro. No real billing adapter or service exists.

## Entitlement preview boundary

`entitlementPreview.ts` is a pure, ephemeral reducer. `EntitlementPreview.svelte` is an isolated
presentation component and is not imported into the running app. Only the test harness mounts
it; the simulated store lives under tests and cannot emit a verified verdict. Neither modifies
`session.entitlement`, AppState, license files, TV state or purchase history. Verified/revoked
reducer events are simulation inputs, not an implemented security boundary or provider verdict.
There is no persisted entitlement, purchase verification service, account system, product, price,
payment call, offline cache policy, tip URL or before-value journal. Production integration
requires the separately reviewed Play verification ([`BILLING-IMPLEMENTATION-PLAN.md`](BILLING-IMPLEMENTATION-PLAN.md))
and Free undo ([`FREE-SETTINGS-UNDO-DESIGN.md`](FREE-SETTINGS-UNDO-DESIGN.md)) designs.

Shared-core tests named `free_user_can_` exercise the real recovery implementations against mock
ADB after an entitlement downgrade (not physical devices). The emergency recovery wrapper delegates
to a private helper so the Free-state path can be called in a unit test. The matrix coverage test
detects added/removed registered commands; it is not a substitute for reviewing future gate
changes or auditing every indirect call chain.
