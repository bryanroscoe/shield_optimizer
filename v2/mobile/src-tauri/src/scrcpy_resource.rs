use std::path::{Path, PathBuf};

const SERVER_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src-tauri/resources/scrcpy-server-v3.1"
));
const SERVER_NAME: &str = "scrcpy-server-v3.1";

/// Materialize the embedded scrcpy server in app-private storage. Rewriting a
/// mismatched file repairs interrupted or stale installs without requiring an
/// application update or external storage permission.
pub fn materialize(data_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("create app data directory: {e}"))?;
    let destination = data_dir.join(SERVER_NAME);
    if matches!(std::fs::read(&destination), Ok(bytes) if bytes == SERVER_BYTES) {
        return Ok(destination);
    }

    let temporary = data_dir.join(format!("{SERVER_NAME}.tmp"));
    std::fs::write(&temporary, SERVER_BYTES)
        .map_err(|e| format!("write embedded scrcpy server: {e}"))?;
    if destination.exists() {
        std::fs::remove_file(&destination)
            .map_err(|e| format!("replace stale scrcpy server: {e}"))?;
    }
    std::fs::rename(&temporary, &destination)
        .map_err(|e| format!("install embedded scrcpy server: {e}"))?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn embedded_server_matches_audited_release() {
        assert_eq!(SERVER_BYTES.len(), 90_640);
        assert_eq!(
            format!("{:x}", Sha256::digest(SERVER_BYTES)),
            "958f0944a62f23b1f33a16e9eb14844c1a04b882ca175a738c16d23cb22b86c0"
        );
    }

    #[test]
    fn materialize_creates_and_repairs_server() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = materialize(dir.path()).expect("materialize server");
        assert_eq!(std::fs::read(&path).expect("read server"), SERVER_BYTES);

        std::fs::write(&path, b"stale").expect("corrupt server");
        let repaired = materialize(dir.path()).expect("repair server");
        assert_eq!(repaired, path);
        assert_eq!(std::fs::read(path).expect("read repair"), SERVER_BYTES);
    }
}
