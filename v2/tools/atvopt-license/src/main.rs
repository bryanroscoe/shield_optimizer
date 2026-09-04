//! Offline license issuer for ATV Optimizer Pro.
//!
//! Signing keys never live in the repo — `keygen` writes the private key to a
//! path you choose (convention: `~/.atvopt/license-signing-key.prod`) and
//! prints only the public half, which is what gets pasted into
//! `crates/core/src/license.rs`. Verification reuses the app's own code, so
//! `verify` answers exactly what a shipped build would.
//!
//! See `mobile/LICENSING.md`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{NaiveDate, Utc};
use clap::{Parser, Subcommand};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use shield_optimizer_core::license::{issue_license, parse_license};

#[derive(Parser)]
#[command(
    name = "atvopt-license",
    about = "Issue and verify signed ATV Optimizer Pro licenses",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a signing key pair. Writes the private key to `--out` and
    /// prints the public key plus the Rust literal to paste into license.rs.
    Keygen {
        #[arg(long)]
        out: PathBuf,
        /// Overwrite an existing key file. Off by default: silently replacing a
        /// production key would invalidate every license signed with it.
        #[arg(long)]
        force: bool,
    },
    /// Sign a new Pro license.
    Issue {
        /// Private key file written by `keygen`.
        #[arg(long)]
        key: PathBuf,
        /// Buyer identity recorded in the license (email or name, <= 40 bytes).
        #[arg(long)]
        licensee: String,
        /// Last day the license is valid, inclusive. Omit for perpetual.
        #[arg(long, value_name = "YYYY-MM-DD")]
        expires: Option<String>,
        /// Which embedded public key verifies this license.
        #[arg(long, default_value_t = 1)]
        key_id: u8,
        /// Issue date, defaults to today (UTC).
        #[arg(long, value_name = "YYYY-MM-DD")]
        issued: Option<String>,
    },
    /// Verify a license string against the keys this build embeds.
    Verify { license: String },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Keygen { out, force } => keygen(&out, force),
        Command::Issue {
            key,
            licensee,
            expires,
            key_id,
            issued,
        } => issue(
            &key,
            &licensee,
            expires.as_deref(),
            key_id,
            issued.as_deref(),
        ),
        Command::Verify { license } => verify(&license),
    }
}

fn keygen(out: &Path, force: bool) -> Result<()> {
    if out.exists() && !force {
        bail!(
            "{} already exists — pass --force only if you are sure no licenses were signed with it",
            out.display()
        );
    }
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
    }
    let signing = SigningKey::generate(&mut OsRng);
    let private_hex = hex(&signing.to_bytes());
    write_private_key(out, &private_hex)?;

    let public = signing.verifying_key().to_bytes();
    println!("private key written to {} (mode 600)", out.display());
    println!("public key: {}", hex(&public));
    println!();
    println!("Paste into crates/core/src/license.rs as DEV_PUBLIC_KEY or PROD_PUBLIC_KEY:");
    println!("[u8; 32] = {};", rust_literal(&public));
    Ok(())
}

/// Write the key 0600 from the start on Unix — never create it world-readable
/// and fix it up afterwards.
fn write_private_key(out: &Path, private_hex: &str) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(out)
        .with_context(|| format!("create {}", out.display()))?;
    writeln!(file, "{private_hex}").with_context(|| format!("write {}", out.display()))
}

fn issue(
    key_path: &Path,
    licensee: &str,
    expires: Option<&str>,
    key_id: u8,
    issued: Option<&str>,
) -> Result<()> {
    let signing_key = read_private_key(key_path)?;
    let issued = match issued {
        Some(s) => parse_date(s)?,
        None => Utc::now().date_naive(),
    };
    let expires = match expires {
        Some(s) => Some(parse_date(s)?),
        None => None,
    };
    let license = issue_license(&signing_key, key_id, licensee, issued, expires)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("{license}");
    Ok(())
}

fn verify(license: &str) -> Result<()> {
    match parse_license(license) {
        Ok(info) => {
            println!("valid");
            println!("  plan:     {:?}", info.plan);
            println!("  licensee: {}", info.licensee);
            println!("  issued:   {}", info.issued);
            println!(
                "  expires:  {}",
                info.expires
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "perpetual".to_string())
            );
            println!("  key id:   {}", info.key_id);
            Ok(())
        }
        Err(e) => bail!("{e}"),
    }
}

fn read_private_key(path: &Path) -> Result<[u8; 32]> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let text = text.trim();
    if text.len() != 64 {
        bail!(
            "{} does not look like a signing key (expected 64 hex characters)",
            path.display()
        );
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
            .with_context(|| format!("{} is not valid hex", path.display()))?;
    }
    Ok(bytes)
}

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("{s} is not a YYYY-MM-DD date"))
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn rust_literal(bytes: &[u8; 32]) -> String {
    let body: Vec<String> = bytes.iter().map(|b| format!("0x{b:02x}")).collect();
    format!("[{}]", body.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shield_optimizer_core::license::public_key_for;

    #[test]
    fn private_key_file_round_trips() {
        let dir = std::env::temp_dir().join("atvopt-license-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("key.test");
        let _ = fs::remove_file(&path);
        keygen(&path, true).unwrap();
        let seed = read_private_key(&path).unwrap();
        let license = issue_license(
            &seed,
            1,
            "buyer@example.com",
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            None,
        )
        .unwrap();
        assert!(license.starts_with("ATVOPT-"));
        assert_eq!(public_key_for(&seed).len(), 32);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn rejects_a_key_file_that_is_not_hex() {
        let dir = std::env::temp_dir().join("atvopt-license-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.test");
        fs::write(&path, "z".repeat(64)).unwrap();
        assert!(read_private_key(&path).is_err());
        fs::write(&path, "short").unwrap();
        assert!(read_private_key(&path).is_err());
        fs::remove_file(&path).unwrap();
    }
}
