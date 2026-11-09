//! Which CPU architectures an APK ships native code for, read from the ZIP central directory,
//! and whether that matches what a TV reports.
//!
//! Pure bytes in, decisions out: the phone reads the end of each staged APK and its central
//! directory and hands the bytes here, so the check needs no ZIP or manifest decoder. It reads
//! entry names only (`lib/<abi>/…`) and never decompresses anything. Desktop's `inspect_apk`
//! uses the same compatibility rule.

/// How much of a file's end can hold the end-of-central-directory record: the 22-byte record
/// plus the longest comment ZIP allows.
pub const EOCD_SEARCH_BYTES: u64 = 22 + u16::MAX as u64;

/// Larger central directories are not read; the ABIs are then reported as unknown.
pub const MAX_CENTRAL_DIRECTORY_BYTES: u64 = 16 * 1024 * 1024;

const EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const CENTRAL_ENTRY_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const CENTRAL_ENTRY_HEADER: usize = 46;

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Where the central directory sits, as `(offset, size)`, from the last bytes of a file
/// (`tail`, at most [`EOCD_SEARCH_BYTES`]) and the file's length. `None` when no consistent
/// record is found, for ZIP64 archives, and for a directory that would run past the file or
/// over [`MAX_CENTRAL_DIRECTORY_BYTES`].
pub fn central_directory_range(tail: &[u8], file_len: u64) -> Option<(u64, u64)> {
    if tail.len() < 22 || tail.len() as u64 > file_len {
        return None;
    }
    let tail_start = file_len - tail.len() as u64;
    // The record is the last one whose comment length reaches exactly to the end of the file.
    (0..=tail.len() - 22).rev().find_map(|at| {
        if tail[at..at + 4] != EOCD_SIGNATURE {
            return None;
        }
        let comment = u16_at(tail, at + 20)? as usize;
        if at + 22 + comment != tail.len() {
            return None;
        }
        let size = u32_at(tail, at + 12)?;
        let offset = u32_at(tail, at + 16)?;
        if size == u32::MAX || offset == u32::MAX {
            return None;
        }
        let (size, offset) = (u64::from(size), u64::from(offset));
        let record_at = tail_start + at as u64;
        (size <= MAX_CENTRAL_DIRECTORY_BYTES && offset.checked_add(size)? <= record_at)
            .then_some((offset, size))
    })
}

/// An ABI directory name as Android uses them (`arm64-v8a`, `armeabi-v7a`, `x86_64`). Anything
/// else under `lib/` is not reported, so a crafted name can't reach the screen.
fn is_abi_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The ABIs (`lib/<abi>/…` entries) listed in a central directory, sorted and deduplicated.
/// Empty means the APK ships no native code. `None` when the directory is malformed.
pub fn native_abis(central_directory: &[u8]) -> Option<Vec<String>> {
    let mut abis: Vec<String> = Vec::new();
    let mut at = 0;
    while at < central_directory.len() {
        if central_directory.get(at..at + 4)? != CENTRAL_ENTRY_SIGNATURE {
            return None;
        }
        let name_len = u16_at(central_directory, at + 28)? as usize;
        let extra_len = u16_at(central_directory, at + 30)? as usize;
        let comment_len = u16_at(central_directory, at + 32)? as usize;
        let name_at = at + CENTRAL_ENTRY_HEADER;
        let name = central_directory.get(name_at..name_at + name_len)?;
        if let Some(abi) = name
            .strip_prefix(b"lib/")
            .and_then(|rest| rest.iter().position(|&b| b == b'/').map(|end| &rest[..end]))
            .and_then(|abi| std::str::from_utf8(abi).ok())
            .filter(|abi| is_abi_name(abi))
        {
            if !abis.iter().any(|known| known == abi) {
                abis.push(abi.to_string());
            }
        }
        at = name_at + name_len + extra_len + comment_len;
    }
    abis.sort();
    Some(abis)
}

/// The TV's `getprop ro.product.cpu.abilist` answer as a list. Blank means it didn't say.
pub fn parse_abilist(stdout: &str) -> Vec<String> {
    stdout
        .trim()
        .split(',')
        .map(|abi| abi.trim().to_string())
        .filter(|abi| !abi.is_empty())
        .collect()
}

/// `Some(false)` only when the APK ships native code and none of it matches what the TV
/// reports. An APK without native code runs anywhere (`Some(true)`); an unreported TV side is
/// `None`, which the UI must not draw as a mismatch.
pub fn abi_compatible(apk_abis: &[String], device_abis: &[String]) -> Option<bool> {
    if apk_abis.is_empty() {
        Some(true)
    } else if device_abis.is_empty() {
        None
    } else {
        Some(apk_abis.iter().any(|abi| device_abis.contains(abi)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored (uncompressed) ZIP with the given entry names and empty bodies.
    fn zip(names: &[&str], comment: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for name in names {
            let offset = out.len() as u32;
            out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
            out.extend_from_slice(&[0; 22]);
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            central.extend_from_slice(&CENTRAL_ENTRY_SIGNATURE);
            central.extend_from_slice(&[0; 24]);
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&[0; 8]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&EOCD_SIGNATURE);
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        out.extend_from_slice(comment);
        out
    }

    fn abis_of(bytes: &[u8]) -> Option<Vec<String>> {
        let len = bytes.len() as u64;
        let tail_len = EOCD_SEARCH_BYTES.min(len) as usize;
        let (offset, size) = central_directory_range(&bytes[bytes.len() - tail_len..], len)?;
        native_abis(&bytes[offset as usize..(offset + size) as usize])
    }

    #[test]
    fn lists_each_native_abi_once_in_order() {
        let apk = zip(
            &[
                "AndroidManifest.xml",
                "lib/x86_64/libmain.so",
                "lib/arm64-v8a/libmain.so",
                "lib/arm64-v8a/libother.so",
                "lib/armeabi-v7a/libmain.so",
                "classes.dex",
            ],
            b"",
        );
        assert_eq!(
            abis_of(&apk).unwrap(),
            ["arm64-v8a", "armeabi-v7a", "x86_64"]
        );
    }

    #[test]
    fn an_apk_without_native_code_lists_none_and_runs_anywhere() {
        let apk = zip(
            &["AndroidManifest.xml", "classes.dex", "res/lib/x.png"],
            b"",
        );
        let abis = abis_of(&apk).unwrap();
        assert!(abis.is_empty());
        assert_eq!(abi_compatible(&abis, &[]), Some(true));
    }

    #[test]
    fn an_archive_comment_is_skipped_and_a_fake_record_inside_it_is_not_trusted() {
        let mut comment = EOCD_SIGNATURE.to_vec();
        comment.extend_from_slice(&[0xff; 18]);
        let apk = zip(&["lib/arm64-v8a/a.so"], &comment);
        assert_eq!(abis_of(&apk).unwrap(), ["arm64-v8a"]);
    }

    #[test]
    fn odd_names_under_lib_are_not_reported() {
        let apk = zip(
            &[
                "lib/../evil/a.so",
                "lib//a.so",
                "lib/a b/x.so",
                "lib/x86",
                "lib/x86/a.so",
            ],
            b"",
        );
        assert_eq!(abis_of(&apk).unwrap(), ["x86"]);
    }

    #[test]
    fn non_zip_truncated_and_inconsistent_input_is_unknown() {
        assert_eq!(
            central_directory_range(b"not a zip at all, just text", 27),
            None
        );
        assert_eq!(central_directory_range(&[], 0), None);
        let apk = zip(&["lib/x86/a.so"], b"");
        // A tail claiming to be longer than the file.
        assert_eq!(central_directory_range(&apk, 10), None);
        // A directory offset past the record.
        let mut bad = apk.clone();
        let at = bad.len() - 6;
        bad[at..at + 4].copy_from_slice(&u32::MAX.wrapping_sub(1).to_le_bytes());
        assert_eq!(abis_of(&bad), None);
        // ZIP64 markers.
        let mut zip64 = apk.clone();
        zip64[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(abis_of(&zip64), None);
        // A central directory that stops mid-entry.
        let (offset, size) = {
            let len = apk.len() as u64;
            central_directory_range(&apk, len).unwrap()
        };
        assert_eq!(
            native_abis(&apk[offset as usize..(offset + size) as usize - 3]),
            None
        );
        assert_eq!(native_abis(b"PK\x03\x04garbage"), None);
        assert_eq!(native_abis(&[]), Some(Vec::new()));
    }

    #[test]
    fn abilist_parses_and_blank_means_unreported() {
        assert_eq!(
            parse_abilist("arm64-v8a,armeabi-v7a, armeabi\n"),
            ["arm64-v8a", "armeabi-v7a", "armeabi"]
        );
        assert!(parse_abilist("  \n").is_empty());
    }

    #[test]
    fn compatibility_claims_a_mismatch_only_when_both_sides_are_known() {
        let arm = vec!["arm64-v8a".to_string()];
        let x86 = vec!["x86_64".to_string()];
        assert_eq!(abi_compatible(&arm, &arm), Some(true));
        assert_eq!(abi_compatible(&x86, &arm), Some(false));
        assert_eq!(abi_compatible(&x86, &[]), None);
        assert_eq!(abi_compatible(&[], &arm), Some(true));
    }
}
