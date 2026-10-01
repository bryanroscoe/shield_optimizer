//! Writes a "Report this app" file to the path the user picked in the save
//! dialog. Nothing is sent anywhere: this is the Save as file button, and the
//! only thing it can write is a report the frontend built and showed first.

use std::path::Path;

use serde_json::Value;

/// One record is a few hundred bytes; the note is capped at 1000 characters.
const MAX_REPORT_BYTES: usize = 16 * 1024;

/// Accept only what `src/lib/app-report.ts` builds: a `schema_version: 1`
/// desktop report holding one installed-package record. Anything else is
/// refused, so this command never becomes a general file writer.
fn validate(path: &Path, contents: &str) -> Result<(), String> {
    let is_json = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    if !is_json {
        return Err("An app report is saved as a .json file.".into());
    }
    if contents.len() > MAX_REPORT_BYTES {
        return Err("This report is larger than an app report can be.".into());
    }
    let root: Value =
        serde_json::from_str(contents).map_err(|_| "This is not an app report.".to_string())?;
    let shaped = root.get("schema_version").and_then(Value::as_u64) == Some(1)
        && root.get("source").and_then(Value::as_str) == Some("desktop_app_report")
        && root
            .get("records")
            .and_then(Value::as_array)
            .is_some_and(|r| {
                r.len() == 1
                    && r[0].get("kind").and_then(Value::as_str) == Some("installed_package")
            });
    if !shaped {
        return Err("This is not an app report.".into());
    }
    Ok(())
}

#[tauri::command]
pub fn save_app_report(path: String, contents: String) -> Result<(), String> {
    let path = Path::new(&path);
    validate(path, &contents)?;
    std::fs::write(path, contents).map_err(|e| format!("Couldn't save the report: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture registry-triage's tests read, so the file this command
    /// accepts and the file the triage tool parses are the same file.
    fn fixture() -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tools/registry-triage/tests/fixtures/app-report/desktop-app-report.json"),
        )
        .unwrap()
    }

    #[test]
    fn writes_a_report_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("report.json");
        save_app_report(target.display().to_string(), fixture()).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), fixture());

        let txt = dir.path().join("report.txt");
        assert!(save_app_report(txt.display().to_string(), fixture()).is_err());
        assert!(!txt.exists());

        for bad in [
            "not json",
            r#"{"schema_version":2,"source":"desktop_app_report","records":[{"kind":"installed_package"}]}"#,
            r#"{"schema_version":1,"records":[{"kind":"installed_package"}]}"#,
            r#"{"schema_version":1,"source":"desktop_app_report","records":[]}"#,
            r#"{"schema_version":1,"source":"desktop_app_report","records":[{"kind":"unresolved_process"}]}"#,
        ] {
            let p = dir.path().join("bad.json");
            assert!(
                save_app_report(p.display().to_string(), bad.into()).is_err(),
                "{bad}"
            );
            assert!(!p.exists());
        }
        let huge = format!("{{\"pad\":\"{}\"}}", "x".repeat(MAX_REPORT_BYTES));
        assert!(validate(&dir.path().join("big.json"), &huge).is_err());
    }
}
