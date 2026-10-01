//! Device profiles: real, scrubbed captures under
//! `crates/core/tests/fixtures/devices/<name>/` (see `v2/e2e/capture-device.sh`)
//! turned into a stateful [`Device`].
//!
//! Captures hold only read-only command output. Anything the capture may not
//! run (`dumpsys display`, `/proc`, `pm has-feature`) is synthesized here from
//! generic templates so every screen has something real-shaped to parse.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::device::{Device, HomeComponent, HomePolicy, Network, Package, Wireless};

/// `device.json`.
#[derive(Debug, Deserialize)]
pub struct ProfileMeta {
    pub name: String,
    pub serial: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub leanback: Option<bool>,
    #[serde(default)]
    pub notes: String,
    /// Extra `pkg/.Activity` HOME components for packages the capture saw
    /// disabled (a disabled app does not answer the HOME query).
    #[serde(default)]
    pub home_components: Vec<String>,
    #[serde(default)]
    pub home_policy: Option<HomePolicy>,
}

/// Directory holding the checked-in device profiles.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/devices")
}

/// Names of every profile under `dir`.
pub fn list_profiles(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("device.json").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

fn read(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.replace("\r\n", "\n"))
}

fn package_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .map(|p| p.rsplit('=').next().unwrap_or(p).trim().to_string())
        .collect()
}

/// Packages the catalog knows declare HOME, with their launch class, for
/// profiles where the capture saw them disabled.
const KNOWN_HOME: &[(&str, &str, i32)] = &[
    (
        "com.google.android.tvlauncher",
        "com.google.android.tvlauncher.MainActivity",
        0,
    ),
    (
        "com.google.android.apps.tv.launcherx",
        "com.google.android.apps.tv.launcherx.home.HomeActivity",
        0,
    ),
    (
        "com.google.android.leanbacklauncher",
        "com.google.android.leanbacklauncher.MainActivity",
        0,
    ),
    (
        "com.google.android.tungsten.setupwraith",
        "com.google.android.tungsten.setupwraith.ui.MainActivity",
        3,
    ),
    (
        "com.amazon.tv.launcher",
        "com.amazon.tv.launcher.ui.HomeActivity_vNext",
        0,
    ),
    (
        "com.spocky.projengmenu",
        "com.spocky.projengmenu.ui.home.MainActivity",
        0,
    ),
    (
        "me.efesser.flauncher",
        "me.efesser.flauncher.MainActivity",
        0,
    ),
    ("com.sweech.launcher", "com.sweech.launcher.MainActivity", 0),
    (
        "com.wolf.firelauncher",
        "com.wolf.firelauncher.MainActivity",
        0,
    ),
    ("com.overdevs.at4k", "com.overdevs.at4k.MainActivity", 0),
];

fn parse_home_components(text: &str) -> Vec<HomeComponent> {
    let mut out = Vec::new();
    let mut priority = 0;
    let mut class: Option<String> = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("priority=") {
            priority = rest
                .split_whitespace()
                .next()
                .and_then(|p| p.parse().ok())
                .unwrap_or(0);
            class = None;
        } else if let Some(name) = t.strip_prefix("name=") {
            if class.is_none() {
                class = Some(name.to_string());
            }
        } else if let Some(pkg) = t.strip_prefix("packageName=") {
            if let Some(c) = class.take() {
                if !out.iter().any(|h: &HomeComponent| h.package == pkg) {
                    out.push(HomeComponent {
                        package: pkg.to_string(),
                        class: c,
                        priority,
                    });
                }
            }
        }
    }
    out
}

fn component_from(short: &str, priority: i32) -> Option<HomeComponent> {
    let (pkg, class) = short.split_once('/')?;
    let class = match class.strip_prefix('.') {
        Some(rest) => format!("{pkg}.{rest}"),
        None => class.to_string(),
    };
    Some(HomeComponent {
        package: pkg.to_string(),
        class,
        priority,
    })
}

fn parse_settings(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn parse_getprop(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once("]: [")?;
            Some((
                k.trim_start_matches('[').to_string(),
                v.trim_end_matches(']').to_string(),
            ))
        })
        .collect()
}

/// Load a captured profile.
pub fn load_profile(dir: &Path) -> Result<Device, String> {
    let meta: ProfileMeta = serde_json::from_str(
        &read(dir, "device.json").ok_or_else(|| format!("{}: no device.json", dir.display()))?,
    )
    .map_err(|e| format!("{}/device.json: {e}", dir.display()))?;
    let mut d = Device::new(&meta.serial);
    d.leanback = meta.leanback;
    d.props = parse_getprop(&read(dir, "getprop.txt").unwrap_or_default());
    d.props.insert("ro.serialno".into(), meta.serial.clone());

    let all = package_names(&read(dir, "pm-list-packages-u.txt").unwrap_or_default());
    let installed = package_names(&read(dir, "pm-list-packages.txt").unwrap_or_default());
    let disabled = package_names(&read(dir, "pm-list-packages-d.txt").unwrap_or_default());
    let system = package_names(&read(dir, "pm-list-packages-s.txt").unwrap_or_default());
    let names = if all.is_empty() {
        installed.clone()
    } else {
        all
    };
    for name in names {
        d.packages.push((
            name.clone(),
            Package {
                system: system.contains(&name),
                enabled: !disabled.contains(&name),
                installed: installed.is_empty() || installed.contains(&name),
            },
        ));
    }

    for (ns, file) in [
        ("global", "settings-global.txt"),
        ("secure", "settings-secure.txt"),
        ("system", "settings-system.txt"),
    ] {
        d.settings.insert(
            ns.into(),
            parse_settings(&read(dir, file).unwrap_or_default()),
        );
    }

    let mut components =
        parse_home_components(&read(dir, "home-query-activities.txt").unwrap_or_default());
    for short in &meta.home_components {
        if let Some(c) = component_from(short, 0) {
            if !components.iter().any(|h| h.package == c.package) {
                components.push(c);
            }
        }
    }
    for (pkg, class, priority) in KNOWN_HOME {
        if d.package(pkg).is_some() && !components.iter().any(|h| h.package == *pkg) {
            components.push(HomeComponent {
                package: pkg.to_string(),
                class: class.to_string(),
                priority: *priority,
            });
        }
    }
    d.home.components = components;
    d.home.policy = meta.home_policy.unwrap_or_default();
    d.home.preferred = read(dir, "home-resolve-activity.txt").and_then(|t| {
        t.lines()
            .map(str::trim)
            .find(|l| l.contains('/') && !l.contains("ResolverActivity"))
            .map(str::to_string)
    });

    for (key, file) in [
        ("dumpsys meminfo", "dumpsys-meminfo.txt"),
        ("dumpsys diskstats", "dumpsys-diskstats.txt"),
        ("dumpsys activity settings", "dumpsys-activity-settings.txt"),
        ("top -b -n 1", "top.txt"),
    ] {
        if let Some(text) = read(dir, file) {
            d.texts.insert(key.into(), text);
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir.join("dumpsys-package")) {
        for e in entries.flatten() {
            let path = e.path();
            if let (Some(stem), Ok(text)) = (
                path.file_stem().and_then(|s| s.to_str()),
                std::fs::read_to_string(&path),
            ) {
                d.texts.insert(format!("dumpsys package {stem}"), text);
            }
        }
    }
    if let Some(size) = read(dir, "wm-size.txt") {
        d.wm_size = parse_wm(&size, "size");
    }
    if let Some(density) = read(dir, "wm-density.txt") {
        d.wm_density = parse_wm(&density, "density");
    }

    d.network = parse_network(
        &meta.serial,
        &read(dir, "adb-devices.txt").unwrap_or_default(),
        &read(dir, "mdns-services.txt").unwrap_or_default(),
    );
    synthesize(&mut d);
    Ok(d)
}

fn parse_wm(text: &str, label: &str) -> (String, Option<String>) {
    let get = |prefix: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(prefix).map(|v| v.trim().to_string()))
    };
    (
        get(&format!("Physical {label}:")).unwrap_or_default(),
        get(&format!("Override {label}:")),
    )
}

fn parse_network(serial: &str, devices: &str, mdns: &str) -> Option<Network> {
    let mut net = Network::default();
    for line in mdns.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        let [instance, service, endpoint] = cols.as_slice() else {
            continue;
        };
        let Some((ip, port)) = endpoint.rsplit_once(':') else {
            continue;
        };
        let port: u16 = port.parse().ok()?;
        net.ip = ip.to_string();
        match *service {
            "_adb._tcp" => {
                net.legacy_port = Some(port);
                net.advertise_legacy = true;
            }
            "_adb-tls-connect._tcp" => {
                net.wireless = Some(Wireless {
                    connect_port: port,
                    connect_instance: instance.to_string(),
                    pairing: None,
                })
            }
            _ => {}
        }
    }
    for line in devices.lines() {
        let Some(key) = line.split_whitespace().next() else {
            continue;
        };
        if let Some(instance) = key.strip_suffix("._adb-tls-connect._tcp") {
            if net.wireless.is_none() {
                net.wireless = Some(Wireless {
                    connect_port: 37_000,
                    connect_instance: instance.to_string(),
                    pairing: None,
                });
            }
        } else if let Some((ip, port)) = key.rsplit_once(':') {
            if let Ok(port) = port.parse::<u16>() {
                net.ip = ip.to_string();
                if port == 5555 {
                    net.legacy_port = Some(port);
                } else if net.wireless.is_none() {
                    net.wireless = Some(Wireless {
                        connect_port: port,
                        connect_instance: format!("adb-{serial}-sim000"),
                        pairing: None,
                    });
                }
            }
        }
    }
    if net.ip.is_empty() {
        net.ip = "192.0.2.10".into();
    }
    (net.legacy_port.is_some() || net.wireless.is_some()).then_some(net)
}

/// The transports a profile was captured under, `(key)`, in order.
pub fn captured_transports(dir: &Path) -> Vec<String> {
    read(dir, "adb-devices.txt")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

/// Fill everything a capture is not allowed to read with generic, parseable
/// output. Values are deliberately unremarkable.
pub fn synthesize(d: &mut Device) {
    let is_tv = d.leanback == Some(true) || d.prop("ro.build.characteristics").contains("tv");
    let (w, h) = d
        .wm_size
        .0
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
        .unwrap_or((1920, 1080));
    d.texts.entry("dumpsys display".into()).or_insert_with(|| {
        format!(
            "DISPLAY MANAGER (dumpsys display)\n  mDisplayDevices:\n    DisplayDeviceInfo{{\"Built-in Screen\": uniqueId=\"local:0\", {w} x {h}, modeId 1, defaultModeId 1, supportedModes [{{id=1, width={w}, height={h}, fps=60.0}}, {{id=2, width={w}, height={h}, fps=50.0}}, {{id=3, width={w}, height={h}, fps=23.976}}], colorMode 0, supportedColorModes [0], HdrCapabilities HdrCapabilities{{mSupportedHdrTypes=[1, 2], mMaxLuminance=500.0, mMaxAverageLuminance=500.0, mMinLuminance=0.0}}, density 320, 320.0 x 320.0 dpi, appVsyncOff 0, presDeadline 16666666, touch NONE, rotation 0, type INTERNAL}}\n  mActiveModeId=1\n"
        )
    });
    d.texts.entry("dumpsys thermalservice".into()).or_insert_with(|| {
        "IsStatusOverride: false\nThermalEventListeners:\nCurrent temperatures from HAL:\n\tTemperature{mValue=46.5, mType=0, mName=CPU, mStatus=0}\n\tTemperature{mValue=41.0, mType=3, mName=SKIN, mStatus=0}\n".into()
    });
    d.texts
        .entry("dumpsys activity settings".into())
        .or_insert_with(|| "ACTIVITY MANAGER SETTINGS (dumpsys activity settings) activity_manager_constants:\n  max_cached_processes=32\n\n  CUR_MAX_CACHED_PROCESSES=32\n  CUR_MAX_EMPTY_PROCESSES=16\n  CUR_TRIM_EMPTY_PROCESSES=8\n".into());
    d.texts.entry("dumpsys audio".into()).or_insert_with(|| {
        if is_tv {
            "Audio event log:\n- STREAM_MUSIC:\n   Devices: hdmi\n".into()
        } else {
            "Audio event log:\n- STREAM_MUSIC:\n   Devices: speaker\n".into()
        }
    });
    d.texts
        .entry("dumpsys hardware_properties".into())
        .or_insert_with(|| "****** Dump of HardwarePropertiesManagerService ******\nCPU temperatures: [46.5]\nGPU temperatures: [44.0]\nBattery temperature: []\nSkin temperatures: [41.0]\n".into());
    d.files.entry("/proc/meminfo".into()).or_insert_with(|| {
        "MemTotal:        3000000 kB\nMemFree:          400000 kB\nMemAvailable:    1500000 kB\n".into()
    });
    if is_tv {
        d.files
            .entry("/vendor/etc/media_codecs.xml".into())
            .or_insert_with(|| MEDIA_CODECS.into());
    }
    d.files
        .entry("/sdcard/Download/notes.txt".into())
        .or_insert_with(|| "simulated file\n".into());
    d.files
        .entry("/sdcard/Movies/clip.mp4".into())
        .or_insert_with(|| "0000".into());
    d.settings
        .entry("global".into())
        .or_default()
        .entry("device_name".into())
        .or_insert_with(|| {
            format!(
                "Test {}",
                d.props.get("ro.product.model").cloned().unwrap_or_default()
            )
        });
}

const MEDIA_CODECS: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<MediaCodecs>
    <Decoders>
        <MediaCodec name="OMX.Nvidia.h264.decode" type="video/avc">
            <Limit name="size" min="16x16" max="3840x2160" />
            <Feature name="adaptive-playback" />
        </MediaCodec>
        <MediaCodec name="OMX.Nvidia.h265.decode" type="video/hevc">
            <Limit name="size" min="16x16" max="3840x2160" />
        </MediaCodec>
        <MediaCodec name="OMX.Nvidia.vp9.decode" type="video/x-vnd.on2.vp9">
            <Limit name="size" min="16x16" max="3840x2160" />
        </MediaCodec>
        <MediaCodec name="OMX.Nvidia.eac3.decoder" type="audio/eac3" />
    </Decoders>
</MediaCodecs>
"#;
