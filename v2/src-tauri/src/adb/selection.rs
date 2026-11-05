//! Which ADB engine runs this session: the built-in Rust transport or Google
//! platform-tools. A launch override beats the saved Settings choice, which
//! beats the build's default.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Launch override. `direct` is the built-in engine, `subprocess` is
/// platform-tools; any other value is a visible configuration error.
pub const TRANSPORT_ENV: &str = "ATV_OPTIMIZER_TRANSPORT";
const SETTING_FILE: &str = "adb-engine.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdbEngine {
    Builtin,
    PlatformTools,
}

impl AdbEngine {
    pub fn label(self) -> &'static str {
        match self {
            Self::Builtin => "built-in",
            Self::PlatformTools => "Google platform-tools",
        }
    }
}

/// Where the selected engine came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineSource {
    Env,
    Saved,
    Default,
}

pub fn build_default() -> AdbEngine {
    if cfg!(feature = "native-adb-default") {
        AdbEngine::Builtin
    } else {
        AdbEngine::PlatformTools
    }
}

pub fn builtin_available() -> bool {
    cfg!(feature = "direct-adb")
}

pub fn parse_override(value: &str) -> Result<AdbEngine, String> {
    match value {
        "subprocess" => Ok(AdbEngine::PlatformTools),
        "direct" => Ok(AdbEngine::Builtin),
        _ => Err(format!("{TRANSPORT_ENV} must be subprocess or direct.")),
    }
}

/// The engine to start, given the launch override and the saved choice.
pub fn resolve(
    env: Option<&str>,
    saved: Option<AdbEngine>,
) -> Result<(AdbEngine, EngineSource), String> {
    if let Some(value) = env {
        return parse_override(value).map(|engine| (engine, EngineSource::Env));
    }
    Ok(match saved {
        Some(engine) => (engine, EngineSource::Saved),
        None => (build_default(), EngineSource::Default),
    })
}

#[derive(Serialize, Deserialize)]
struct SavedEngine {
    engine: AdbEngine,
}

fn setting_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTING_FILE)
}

/// The saved choice. A file that cannot be read or parsed counts as no
/// choice, so a damaged setting can never keep the app from starting.
pub fn load_saved(data_dir: &Path) -> Option<AdbEngine> {
    let path = setting_path(data_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "could not read the saved ADB engine; using the build default");
            return None;
        }
    };
    match serde_json::from_slice::<SavedEngine>(&bytes) {
        Ok(saved) => Some(saved.engine),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "saved ADB engine is not valid; using the build default");
            None
        }
    }
}

/// Save the choice whole or not at all: written beside the setting, then
/// renamed over it.
pub fn save(data_dir: &Path, engine: AdbEngine) -> Result<(), String> {
    let path = setting_path(data_dir);
    let result = (|| {
        std::fs::create_dir_all(data_dir)?;
        let json = serde_json::to_vec(&SavedEngine { engine }).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)
    })();
    result.map_err(|e| {
        tracing::warn!(error = %e, "could not save the ADB engine setting");
        format!("Couldn't save the ADB engine setting: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launch_override_maps_to_an_engine_and_rejects_anything_else() {
        assert_eq!(parse_override("direct").unwrap(), AdbEngine::Builtin);
        assert_eq!(
            parse_override("subprocess").unwrap(),
            AdbEngine::PlatformTools
        );
        for invalid in ["automatic", "", "Direct", "builtin"] {
            assert_eq!(
                parse_override(invalid).unwrap_err(),
                "ATV_OPTIMIZER_TRANSPORT must be subprocess or direct."
            );
        }
    }

    #[test]
    fn override_beats_saved_which_beats_the_build_default() {
        assert_eq!(
            resolve(None, None).unwrap(),
            (
                if cfg!(feature = "native-adb-default") {
                    AdbEngine::Builtin
                } else {
                    AdbEngine::PlatformTools
                },
                EngineSource::Default
            )
        );
        for saved in [AdbEngine::Builtin, AdbEngine::PlatformTools] {
            assert_eq!(
                resolve(None, Some(saved)).unwrap(),
                (saved, EngineSource::Saved)
            );
        }
        assert_eq!(
            resolve(Some("subprocess"), Some(AdbEngine::Builtin)).unwrap(),
            (AdbEngine::PlatformTools, EngineSource::Env)
        );
        assert_eq!(
            resolve(Some("direct"), Some(AdbEngine::PlatformTools)).unwrap(),
            (AdbEngine::Builtin, EngineSource::Env)
        );
        assert!(resolve(Some("automatic"), Some(AdbEngine::PlatformTools)).is_err());
    }

    #[test]
    fn normal_builds_keep_platform_tools_as_the_default() {
        assert_eq!(
            build_default() == AdbEngine::Builtin,
            cfg!(feature = "native-adb-default")
        );
        assert_eq!(builtin_available(), cfg!(feature = "direct-adb"));
    }

    #[test]
    fn a_saved_choice_round_trips_and_replaces_the_previous_one() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_saved(dir.path()), None);
        save(dir.path(), AdbEngine::Builtin).unwrap();
        assert_eq!(load_saved(dir.path()), Some(AdbEngine::Builtin));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("adb-engine.json")).unwrap(),
            r#"{"engine":"builtin"}"#
        );
        save(dir.path(), AdbEngine::PlatformTools).unwrap();
        assert_eq!(load_saved(dir.path()), Some(AdbEngine::PlatformTools));
        assert!(!dir.path().join("adb-engine.json.tmp").exists());
    }

    #[test]
    fn a_damaged_setting_counts_as_unset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("adb-engine.json");
        for body in [
            "",
            "not json",
            r#"{"engine":"automatic"}"#,
            r#"{"other":1}"#,
            "[]",
        ] {
            std::fs::write(&path, body).unwrap();
            assert_eq!(load_saved(dir.path()), None, "{body:?}");
        }
        std::fs::remove_file(&path).unwrap();
        // Unreadable rather than malformed.
        std::fs::create_dir(&path).unwrap();
        assert_eq!(load_saved(dir.path()), None);
    }

    #[test]
    fn a_failed_save_reports_and_leaves_the_previous_choice() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), AdbEngine::Builtin).unwrap();
        std::fs::create_dir(dir.path().join("adb-engine.json.tmp")).unwrap();
        let error = save(dir.path(), AdbEngine::PlatformTools).unwrap_err();
        assert!(error.starts_with("Couldn't save the ADB engine setting"));
        assert_eq!(load_saved(dir.path()), Some(AdbEngine::Builtin));
    }

    #[test]
    fn desktop_bundle_retains_the_vendored_mit_notice() {
        assert_eq!(
            include_str!("../../resources/direct-adb-MIT-LICENSE.txt"),
            include_str!("../../../vendor/adb_client/LICENSE")
        );
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert!(config["bundle"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "resources/direct-adb-MIT-LICENSE.txt"));
    }
}
