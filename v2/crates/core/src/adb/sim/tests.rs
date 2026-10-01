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
