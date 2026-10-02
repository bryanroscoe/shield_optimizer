use std::sync::Arc;

use super::*;
use crate::commands::{devices, launcher, AppState};
use crate::engine::AppListBundle;

fn profile(name: &str) -> Device {
    load_profile(&fixtures_dir().join(name)).expect("profile loads")
}

fn state(sim: &SimulatedAdb) -> AppState {
    AppState::new(
        Arc::new(sim.clone()),
        AppListBundle::default(),
        tempfile::tempdir().unwrap().keep(),
    )
    .with_entitlement(crate::license::Entitlement::Pro)
}

fn shield_world() -> SimulatedAdb {
    let sim = SimulatedAdb::empty();
    {
        let mut w = sim.world();
        let d = profile("shield-tv-pro");
        let serial = d.serial.clone();
        w.add_device(d);
        w.attach("192.0.2.1:5555", &serial, TransportState::Device);
    }
    sim
}

#[tokio::test]
async fn shield_profile_lists_as_a_shield() {
    let sim = shield_world();
    let st = state(&sim);
    let list = devices::list_devices_impl(&st).await.unwrap();
    assert_eq!(list.len(), 1);
    let d = &list[0];
    assert_eq!(d.serial, "192.0.2.1:5555");
    assert_eq!(
        d.properties.as_ref().unwrap().serial_number,
        "1324619053514"
    );
    assert_eq!(d.properties.as_ref().unwrap().leanback, Some(true));
    assert!(sim.world().gaps.is_empty(), "{:?}", sim.world().gaps);
}

#[tokio::test]
async fn a_phone_answering_no_to_leanback_still_profiles() {
    let sim = SimulatedAdb::empty();
    {
        let mut w = sim.world();
        let d = profile("pixel-10-pro");
        let serial = d.serial.clone();
        w.add_device(d);
        w.attach("192.0.2.1:41235", &serial, TransportState::Device);
    }
    let st = state(&sim);
    let list = devices::list_devices_impl(&st).await.unwrap();
    assert_eq!(list[0].properties.as_ref().unwrap().leanback, Some(false));
    // The bare read exits 1, as `pm has-feature` does on a device.
    let out = sim
        .shell(
            "192.0.2.1:41235",
            "pm has-feature android.software.leanback",
        )
        .await;
    assert!(out.is_err());
}

#[tokio::test]
async fn forget_drops_every_transport_of_one_device() {
    let sim = shield_world();
    {
        let mut w = sim.world();
        w.attach(
            "adb-1324619053514-abc._adb-tls-connect._tcp",
            "1324619053514",
            TransportState::Device,
        );
    }
    let st = state(&sim);
    assert_eq!(devices::list_devices_impl(&st).await.unwrap().len(), 1);
    let result = devices::forget_device_impl(&st, "192.0.2.1:5555")
        .await
        .unwrap();
    assert_eq!(result.disconnected.len(), 2);
    assert!(sim.world().transports.is_empty());
}

#[tokio::test]
async fn launcher_rows_are_home_handlers_and_switching_moves_home() {
    let sim = shield_world();
    let st = state(&sim);
    let rows = launcher::list_launchers_impl(&st, "192.0.2.1:5555")
        .await
        .unwrap();
    assert!(!rows.is_empty());
    let stock = super::stock_launchers();
    let home = sim
        .world()
        .devices
        .get_mut("1324619053514")
        .unwrap()
        .resolve_home(&stock);
    assert_eq!(
        home.as_deref(),
        Some("com.spocky.projengmenu/.ui.home.MainActivity")
    );
    // Disabling the only real launcher leaves Settings' fallback.
    sim.world()
        .setup_shell(
            "1324619053514",
            "pm disable-user --user 0 com.spocky.projengmenu",
        )
        .unwrap();
    let home = sim
        .world()
        .devices
        .get_mut("1324619053514")
        .unwrap()
        .resolve_home(&stock);
    assert_eq!(
        home.as_deref(),
        Some("com.android.tv.settings/.system.FallbackHome")
    );
}

#[tokio::test]
async fn faults_fire_on_one_batched_section() {
    let sim = shield_world();
    sim.world().faults.push(FaultRule {
        serial: None,
        scope: FaultScope::Shell,
        matches: "getprop ro.build.version.sdk".into(),
        effect: FaultEffect::Output {
            stdout: "99\n".into(),
            exit_code: 0,
        },
        times: Some(1),
        after: 0,
        fired: 0,
    });
    let st = state(&sim);
    let d = devices::list_devices_impl(&st).await.unwrap();
    assert_eq!(d[0].properties.as_ref().unwrap().sdk_level, "99");
    let d = devices::list_devices_impl(&st).await.unwrap();
    assert_eq!(d[0].properties.as_ref().unwrap().sdk_level, "30");
}

// Measured on a Shield TV Pro (Android 11) for #99: writing the global key
// left Android's cached-process limit at 32.
#[tokio::test]
async fn the_background_process_limit_key_does_not_move_the_real_limit() {
    use crate::commands::tuning::get_tweaks_for;
    let sim = shield_world();
    let before = get_tweaks_for(&sim, "192.0.2.1:5555").await.unwrap();
    assert_eq!(before.background_process_limit.as_deref(), Some("2"));
    assert_eq!(before.cached_process_limit, Some(32));
    sim.shell(
        "192.0.2.1:5555",
        "settings put global background_process_limit 1",
    )
    .await
    .unwrap();
    let after = get_tweaks_for(&sim, "192.0.2.1:5555").await.unwrap();
    assert_eq!(after.background_process_limit.as_deref(), Some("1"));
    assert_eq!(after.cached_process_limit, Some(32));
    assert!(sim.world().gaps.is_empty(), "{:?}", sim.world().gaps);
}

#[tokio::test]
async fn unknown_commands_are_named() {
    let sim = shield_world();
    let err = sim
        .shell("192.0.2.1:5555", "frobnicate --all")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("frobnicate --all"), "{err}");
    assert_eq!(sim.world().gaps, vec!["frobnicate --all".to_string()]);
}

#[tokio::test]
async fn a_replayed_screenshot_is_a_png() {
    let sim = shield_world();
    let session = r#"{"v":1,"kind":"adb","args":["-s","192.0.2.1:5555","exec-out","screencap","-p"],"exit_code":0,"stdout":"<2048 bytes>","stderr":"","stream":"bytes"}"#;
    sim.world().replay = Some(replay::Replay::new(&replay::parse_session(session)));
    let png = sim
        .raw_bytes(&["-s", "192.0.2.1:5555", "exec-out", "screencap", "-p"])
        .await
        .unwrap();
    assert_eq!(&png[..4], b"\x89PNG");
    assert!(sim.world().replay.as_ref().unwrap().report().is_empty());
}

/// #122 on v2.3.0: stock disabled, Setup Wraith still enabled with a higher
/// HOME filter priority, and Monet set as default. Monet holds the HOME role
/// and the Home key opens it, but `resolve-activity` still names Setup
/// Wraith. The current launcher is the role holder.
#[tokio::test]
async fn setup_wraith_outranking_the_role_holder_is_not_the_current_launcher() {
    const WRAITH: &str = "com.google.android.tungsten.setupwraith";
    const MONET: &str = "com.klevico.monet";
    let sim = shield_world();
    {
        let mut w = sim.world();
        let d = w.devices.get_mut("1324619053514").unwrap();
        d.props.insert("ro.build.version.sdk".into(), "31".into());
        d.home.policy = HomePolicy::PriorityResolver;
        d.add_home_app(WRAITH, &format!("{WRAITH}.ui.MainActivity"), 3);
        d.add_home_app(MONET, &format!("{MONET}.MainActivity"), 0);
        assert!(!d.package("com.google.android.tvlauncher").unwrap().enabled);
    }
    let st = state(&sim);
    let serial = "192.0.2.1:5555";

    let res =
        launcher::set_default_launcher_impl(&st, serial, MONET, false, &launcher::Progress::Silent)
            .await
            .unwrap();
    assert!(res.ok, "{:?} {:?}", res.last_error, res.diagnostics);

    let stock = super::stock_launchers();
    {
        let mut w = sim.world();
        let d = w.devices.get_mut("1324619053514").unwrap();
        assert_eq!(d.home.role_holder.as_deref(), Some(MONET));
        // The resolver alone, which 2.3.0 displayed, still names Setup Wraith.
        assert!(d.resolve_home(&stock).unwrap().starts_with(WRAITH));
    }

    let adb = st.adb_snapshot().await;
    let reading = launcher::read_current_home(&*adb, serial).await.unwrap();
    assert_eq!(reading.package.as_deref(), Some(MONET));
    assert!(reading.note.unwrap().contains(WRAITH));

    // With Projectivy gone, Monet is the last real launcher: Setup Wraith
    // doesn't count as a Home.
    sim.world()
        .setup_shell(
            "1324619053514",
            "pm disable-user --user 0 com.spocky.projengmenu",
        )
        .unwrap();
    let refused = crate::commands::apps::disable_package_impl(&st, serial, MONET)
        .await
        .unwrap();
    assert!(!refused.ok, "{}", refused.message);
    assert!(
        sim.world()
            .devices
            .get("1324619053514")
            .unwrap()
            .package(MONET)
            .unwrap()
            .enabled
    );

    // The setup helper is never accepted as the default.
    let wraith =
        launcher::set_default_launcher_impl(&st, serial, WRAITH, true, &launcher::Progress::Silent)
            .await
            .unwrap();
    assert!(!wraith.ok);
}

const WRAITH_PKG: &str = "com.google.android.tungsten.setupwraith";
const GTV_HOME: &str = "com.google.android.apps.tv.launcherx";
const PROJECTIVY_PKG: &str = "com.spocky.projengmenu";
const SHIELD_SERIAL: &str = "1324619053514";

/// The reporter's onn 4K Pro (#122): stock Google TV Home enabled, Setup
/// Wraith enabled with the highest HOME priority, and Projectivy installed.
fn google_tv_world(with_wraith: bool) -> SimulatedAdb {
    let sim = shield_world();
    {
        let mut w = sim.world();
        let d = w.devices.get_mut(SHIELD_SERIAL).unwrap();
        d.props.insert("ro.build.version.sdk".into(), "31".into());
        d.home.policy = HomePolicy::StockThenPriority;
        d.add_home_app(GTV_HOME, &format!("{GTV_HOME}.home.HomeActivity"), 0);
        if with_wraith {
            d.add_home_app(WRAITH_PKG, &format!("{WRAITH_PKG}.ui.MainActivity"), 3);
        }
        d.add_home_app(
            PROJECTIVY_PKG,
            &format!("{PROJECTIVY_PKG}.ui.home.MainActivity"),
            0,
        );
    }
    sim
}

async fn pick_projectivy_and_disable_stock(sim: &SimulatedAdb) -> launcher::SetLauncherResult {
    let st = state(sim);
    let serial = "192.0.2.1:5555";
    launcher::set_home_any_impl(&st, serial, PROJECTIVY_PKG, None)
        .await
        .unwrap();
    launcher::disable_stock_launcher_impl(&st, serial, PROJECTIVY_PKG, &launcher::Progress::Silent)
        .await
        .unwrap()
}

fn enabled(sim: &SimulatedAdb, pkg: &str) -> bool {
    sim.world()
        .devices
        .get(SHIELD_SERIAL)
        .unwrap()
        .package(pkg)
        .unwrap()
        .enabled
}

/// #122: with stock disabled, Setup Wraith takes the Home key back unless the
/// takeover turns it off too.
#[tokio::test]
async fn google_tv_takeover_disables_setup_wraith_with_stock() {
    let sim = google_tv_world(true);
    let res = pick_projectivy_and_disable_stock(&sim).await;
    assert!(res.ok, "{:?} {:?}", res.last_error, res.diagnostics);
    assert!(!enabled(&sim, GTV_HOME));
    assert!(!enabled(&sim, WRAITH_PKG));
    let stock = super::stock_launchers();
    let mut w = sim.world();
    let d = w.devices.get_mut(SHIELD_SERIAL).unwrap();
    assert!(d.resolve_home(&stock).unwrap().starts_with(PROJECTIVY_PKG));
}

/// The simulator models the reporter's failure: without the paired disable,
/// Setup Wraith wins the resolver once stock is gone.
#[tokio::test]
async fn google_tv_without_the_paired_disable_wraith_takes_home_back() {
    let sim = google_tv_world(true);
    sim.world()
        .setup_shell(
            SHIELD_SERIAL,
            &format!("pm disable-user --user 0 {GTV_HOME}"),
        )
        .unwrap();
    let stock = super::stock_launchers();
    let mut w = sim.world();
    let d = w.devices.get_mut(SHIELD_SERIAL).unwrap();
    assert!(d.resolve_home(&stock).unwrap().starts_with(WRAITH_PKG));
}

/// A failure while turning Setup Wraith off re-enables stock and Setup Wraith.
#[tokio::test]
async fn google_tv_takeover_failure_re_enables_stock_and_wraith() {
    let sim = google_tv_world(true);
    sim.world().faults.push(FaultRule {
        serial: None,
        scope: FaultScope::Shell,
        matches: format!("pm disable-user --user 0 {WRAITH_PKG}"),
        effect: FaultEffect::Fail {
            stdout: String::new(),
            stderr: "java.lang.SecurityException: Permission denial\n".into(),
            exit_code: 255,
        },
        times: None,
        after: 0,
        fired: 0,
    });
    let res = pick_projectivy_and_disable_stock(&sim).await;
    assert!(!res.ok);
    assert!(enabled(&sim, GTV_HOME), "stock re-enabled");
    assert!(enabled(&sim, WRAITH_PKG), "Setup Wraith re-enabled");
}

/// A device without Setup Wraith behaves exactly as before: no command ever
/// names the setup helper.
#[tokio::test]
async fn takeover_without_setup_wraith_never_touches_it() {
    let sim = google_tv_world(false);
    let res = pick_projectivy_and_disable_stock(&sim).await;
    assert!(res.ok, "{:?} {:?}", res.last_error, res.diagnostics);
    assert!(!format!("{:?}", sim.world().log).contains("setupwraith"));
    assert!(!res.diagnostics.iter().any(|l| l.contains("setupwraith")));
}

/// Shield path: stock is `com.google.android.tvlauncher`, which pairs with
/// nothing, so the takeover never names the setup helper.
#[tokio::test]
async fn shield_takeover_is_unchanged() {
    let sim = shield_world();
    {
        let mut w = sim.world();
        let d = w.devices.get_mut(SHIELD_SERIAL).unwrap();
        d.home.policy = HomePolicy::StockOverrides;
    }
    sim.world()
        .setup_shell(SHIELD_SERIAL, "pm enable com.google.android.tvlauncher")
        .unwrap();
    let res = pick_projectivy_and_disable_stock(&sim).await;
    assert!(res.ok, "{:?} {:?}", res.last_error, res.diagnostics);
    assert!(!format!("{:?}", sim.world().log).contains("setupwraith"));
}

/// The one-click fix: refuses when no real launcher is left, otherwise
/// disables Setup Wraith and keeps Home on a launcher.
#[tokio::test]
async fn disable_setup_helper_is_guarded_and_verified() {
    let sim = google_tv_world(true);
    let st = state(&sim);
    let serial = "192.0.2.1:5555";
    sim.world()
        .setup_shell(
            SHIELD_SERIAL,
            &format!("pm disable-user --user 0 {GTV_HOME}"),
        )
        .unwrap();
    sim.world()
        .setup_shell(
            SHIELD_SERIAL,
            &format!("pm disable-user --user 0 {PROJECTIVY_PKG}"),
        )
        .unwrap();
    let refused = launcher::disable_setup_helper_impl(&st, serial, WRAITH_PKG)
        .await
        .unwrap();
    assert!(!refused.ok, "{}", refused.message);
    assert!(enabled(&sim, WRAITH_PKG));

    sim.world()
        .setup_shell(SHIELD_SERIAL, &format!("pm enable {PROJECTIVY_PKG}"))
        .unwrap();
    let ok = launcher::disable_setup_helper_impl(&st, serial, WRAITH_PKG)
        .await
        .unwrap();
    assert!(ok.ok, "{}", ok.message);
    assert!(!enabled(&sim, WRAITH_PKG));

    let not_helper = launcher::disable_setup_helper_impl(&st, serial, PROJECTIVY_PKG)
        .await
        .unwrap();
    assert!(!not_helper.ok);
    assert!(enabled(&sim, PROJECTIVY_PKG));
}
