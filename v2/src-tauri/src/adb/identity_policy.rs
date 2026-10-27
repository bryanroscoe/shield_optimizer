//! Conservative policy shared by the Win32 implementation and host-only tests.
pub(super) const FULL_FILE_ACCESS: u32 = 0x001f01ff;
#[derive(Clone, Copy)]
pub(super) struct AcePolicy {
    pub allow_current_user: bool,
    pub flags: u8,
    pub mask: u32,
}
pub(super) fn private_acl(
    owner_is_user: bool,
    protected: bool,
    present: bool,
    aces: &[AcePolicy],
) -> bool {
    owner_is_user
        && protected
        && present
        && aces.len() == 1
        && aces[0].allow_current_user
        && aces[0].flags == 0
        && aces[0].mask == FULL_FILE_ACCESS
}
pub(super) fn local_components(path: &str) -> Result<(String, Vec<String>), String> {
    let path = path.replace('/', "\\");
    let bytes = path.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return Err("Native identity requires an absolute local drive path; UNC/device/relative paths are refused.".into());
    }
    let mut parts = Vec::new();
    for part in path[3..].split('\\') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part.chars().any(|c| c < ' ' || "<>:\"|?*".contains(c))
        {
            return Err("Native identity path contains an unsafe Windows component.".into());
        }
        let stem = part.split('.').next().unwrap().to_ascii_uppercase();
        let stem = stem.trim_end_matches([' ', '.']);
        let port = stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"));
        let reserved = matches!(
            stem,
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
        ) || port.is_some_and(|number| {
            matches!(
                number,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        });
        if reserved {
            return Err("Native identity path contains a reserved Windows device name.".into());
        }
        parts.push(part.into());
    }
    if parts.is_empty() {
        return Err("Native identity requires an application-data directory.".into());
    }
    Ok((
        format!("{}:\\", char::from(bytes[0]).to_ascii_uppercase()),
        parts,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_acl_refuses_extra_inherited_wrong_owner_and_null_permissions() {
        let user = AcePolicy {
            allow_current_user: true,
            flags: 0,
            mask: FULL_FILE_ACCESS,
        };
        assert!(private_acl(true, true, true, &[user]));
        for (owner, protected, present) in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
        ] {
            assert!(!private_acl(owner, protected, present, &[user]));
        }
        assert!(!private_acl(true, true, true, &[]));
        assert!(!private_acl(true, true, true, &[user, user]));
        for ace in [
            AcePolicy {
                flags: 0x10,
                ..user
            },
            AcePolicy {
                allow_current_user: false,
                ..user
            },
            AcePolicy {
                mask: 0x120089,
                ..user
            },
        ] {
            assert!(!private_acl(true, true, true, &[ace]));
        }
    }
    #[test]
    fn local_path_policy_refuses_namespaces_aliases_and_alternate_streams() {
        assert_eq!(
            local_components("C:\\Users\\Owner\\AppData")
                .unwrap()
                .1
                .len(),
            3
        );
        for path in [
            "relative",
            "C:relative",
            "\\\\host\\share\\app",
            "\\\\?\\C:\\app",
            "\\\\.\\C:\\app",
            "C:\\app:secret",
            "C:\\app.",
            "C:\\app ",
            "C:\\..\\app",
            "C:\\CON",
            "C:\\NUL.txt",
            "C:\\COM1",
            "C:\\CONIN$",
            "C:\\CONOUT$",
            "C:\\COM¹",
            "C:\\LPT².txt",
            "C:\\a\\\\b",
        ] {
            assert!(local_components(path).is_err(), "{path}");
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ParentAcePolicy {
    pub trusted_writer: bool,
    pub flags: u8,
    pub mask: u32,
}
pub(super) fn parent_acl(owner_is_user: bool, present: bool, aces: &[ParentAcePolicy]) -> bool {
    const MUTATION: u32 = 0x500d0156;
    owner_is_user
        && present
        && aces
            .iter()
            .all(|ace| ace.flags & 0x08 != 0 || ace.trusted_writer || ace.mask & MUTATION == 0)
}
#[cfg(test)]
mod parent_tests {
    use super::*;
    #[test]
    fn inherited_trusted_writers_and_readonly_others_are_accepted() {
        let trusted = ParentAcePolicy {
            trusted_writer: true,
            flags: 0x10,
            mask: FULL_FILE_ACCESS,
        };
        let readonly = ParentAcePolicy {
            trusted_writer: false,
            flags: 0x10,
            mask: 0x1200a9,
        };
        assert!(parent_acl(
            true,
            true,
            &[trusted, trusted, trusted, readonly]
        ));
        assert!(parent_acl(
            true,
            true,
            &[ParentAcePolicy {
                trusted_writer: false,
                flags: 0x08,
                mask: FULL_FILE_ACCESS
            }]
        ));
    }
    #[test]
    fn every_untrusted_mutation_right_null_acl_and_wrong_owner_are_refused() {
        for mask in [
            0x2, 0x4, 0x10, 0x40, 0x100, 0x10000, 0x40000, 0x80000, 0x10000000, 0x40000000,
        ] {
            assert!(!parent_acl(
                true,
                true,
                &[ParentAcePolicy {
                    trusted_writer: false,
                    flags: 0,
                    mask
                }]
            ));
        }
        assert!(!parent_acl(false, true, &[]));
        assert!(!parent_acl(true, false, &[]));
    }
}

pub(super) fn ancestor_acl(
    owner_is_trusted: bool,
    present: bool,
    aces: &[ParentAcePolicy],
) -> bool {
    const REPLACEMENT: u32 = 0x500d0150;
    owner_is_trusted
        && present
        && aces
            .iter()
            .all(|ace| ace.flags & 0x08 != 0 || ace.trusted_writer || ace.mask & REPLACEMENT == 0)
}
#[cfg(test)]
mod ancestor_tests {
    use super::*;
    #[test]
    fn ancestor_sibling_creation_is_distinct_from_replacement() {
        let sibling = ParentAcePolicy {
            trusted_writer: false,
            flags: 0,
            mask: 0x6,
        };
        assert!(ancestor_acl(true, true, &[sibling]));
        assert!(!parent_acl(true, true, &[sibling]));
        for mask in [
            0x10, 0x40, 0x100, 0x10000, 0x40000, 0x80000, 0x10000000, 0x40000000,
        ] {
            assert!(!ancestor_acl(
                true,
                true,
                &[ParentAcePolicy { mask, ..sibling }]
            ));
        }
        assert!(!ancestor_acl(true, false, &[]));
        assert!(!ancestor_acl(
            false,
            true,
            &[ParentAcePolicy {
                mask: 0x1200a9,
                ..sibling
            }]
        ));
    }
}
