// Derived in part from Kadb (Apache-2.0): core/AdbProtocol.kt and core/AdbConnection.kt.

use std::collections::{BTreeMap, HashSet};

use crate::{Result, RustADBError};

/// Original protocol revision: every payload carries a checksum.
pub(crate) const A_VERSION_MIN: u32 = 0x0100_0000;
/// From this revision on, senders write a zero checksum and receivers skip it.
pub(crate) const A_VERSION_SKIP_CHECKSUM: u32 = 0x0100_0001;
/// The revision we send in CNXN. Staying at the original revision makes adbd
/// keep computing checksums, which is what our reader verifies, so the two
/// directions can never disagree about whether `data_crc32` is meaningful.
pub(crate) const A_VERSION: u32 = A_VERSION_MIN;
/// AOSP's host always answers STLS with this fixed value.
pub(crate) const A_STLS_VERSION: u32 = 0x0100_0000;

/// Connect and auth packets go out before either side knows the other's
/// limit, so AOSP caps them at the oldest protocol's payload size.
pub(crate) const MAX_PAYLOAD_V1: usize = 4 * 1024;
/// Our receive limit, advertised in CNXN. It is also the bound on an incoming
/// payload before negotiation, so a corrupt length can never make us allocate
/// more than this.
pub(crate) const MAX_PAYLOAD: u32 = 1024 * 1024;

/// Features this client advertises. Each is client-driven: adbd only offers
/// the matching service, and nothing changes until the client asks for it.
/// `delayed_ack` is deliberately absent; advertising it obliges the client to
/// implement the receive-window accounting on every stream.
pub(crate) const HOST_FEATURES: &[&str] = &[
    "shell_v2",
    "cmd",
    "stat_v2",
    "ls_v2",
    "sendrecv_v2",
    "abb_exec",
];

/// The CNXN payload: `host::features=<csv>`. No trailing NUL, because adbd
/// parses the payload as a string and a NUL would become part of the last
/// feature name.
pub(crate) fn host_banner() -> Vec<u8> {
    format!("host::features={}", HOST_FEATURES.join(",")).into_bytes()
}

/// What the CNXN exchange settled for one connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionInfo {
    protocol_version: u32,
    max_payload: usize,
    features: HashSet<String>,
    device_features: HashSet<String>,
    system_type: String,
    properties: BTreeMap<String, String>,
}

impl ConnectionInfo {
    /// The state before a device has answered CNXN.
    pub(crate) fn unnegotiated() -> Self {
        Self {
            protocol_version: A_VERSION_MIN,
            max_payload: MAX_PAYLOAD_V1,
            features: HashSet::new(),
            device_features: HashSet::new(),
            system_type: String::new(),
            properties: BTreeMap::new(),
        }
    }

    /// Settle the connection from the device's CNXN, as AOSP's
    /// `atransport::update_version` and `parse_banner` do: the protocol
    /// version and payload limit are the smaller of both sides', and a feature
    /// is usable only when both sides list it.
    pub(crate) fn negotiate(
        peer_version: u32,
        peer_max_payload: u32,
        banner: &[u8],
    ) -> Result<Self> {
        if peer_max_payload == 0 {
            return Err(RustADBError::ProtocolError(
                "device advertised a maximum payload of 0".to_string(),
            ));
        }
        let max_payload = usize::try_from(peer_max_payload.min(MAX_PAYLOAD))?;

        let banner = String::from_utf8_lossy(banner);
        // Pre-N devices NUL-terminate the banner.
        let banner = banner.trim_end_matches('\0');
        let mut pieces = banner.splitn(3, ':');
        let system_type = pieces.next().unwrap_or_default().to_string();
        let _serial = pieces.next();
        let mut properties = BTreeMap::new();
        let mut device_features = HashSet::new();
        // AOSP splits the whole banner on ':' and would drop every property
        // after a value that contains one; taking the remainder keeps them.
        if let Some(props) = pieces.next() {
            for prop in props.split(';') {
                let Some((key, value)) = prop.split_once('=') else {
                    continue;
                };
                if key.is_empty() {
                    continue;
                }
                if key == "features" {
                    device_features = value
                        .split(',')
                        .map(str::trim)
                        .filter(|feature| !feature.is_empty())
                        .map(str::to_string)
                        .collect();
                } else if !value.is_empty() {
                    properties.insert(key.to_string(), value.to_string());
                }
            }
        }
        let features = HOST_FEATURES
            .iter()
            .filter(|feature| device_features.contains(**feature))
            .map(|feature| (*feature).to_string())
            .collect();

        Ok(Self {
            protocol_version: peer_version.min(A_VERSION),
            max_payload,
            features,
            device_features,
            system_type,
            properties,
        })
    }

    /// The negotiated protocol revision, `min(device, ours)`.
    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    /// Largest payload one message may carry in either direction, `min(device,
    /// ours)`. A `WRTE` larger than this is refused locally rather than sent,
    /// because adbd drops the whole connection on an oversized packet.
    #[must_use]
    pub const fn max_payload(&self) -> usize {
        self.max_payload
    }

    /// Features both this client and the device support. Gate optional
    /// protocols (sync v2, `abb_exec`, ...) on this set.
    #[must_use]
    pub const fn features(&self) -> &HashSet<String> {
        &self.features
    }

    /// Whether both sides support `feature`.
    #[must_use]
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.contains(feature)
    }

    /// Every feature the device listed, including device-side behavior flags
    /// such as `fixed_push_mkdir` that the client never advertises.
    #[must_use]
    pub const fn device_features(&self) -> &HashSet<String> {
        &self.device_features
    }

    /// The banner's connection type: `device`, `recovery`, `sideload`, ...
    #[must_use]
    pub fn system_type(&self) -> &str {
        &self.system_type
    }

    /// A banner property such as `ro.product.model`.
    #[must_use]
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    pub(crate) const fn verifies_checksums(&self) -> bool {
        self.protocol_version < A_VERSION_SKIP_CHECKSUM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODERN: &[u8] = b"device::ro.product.name=mdarcy;ro.product.model=SHIELD Android TV;\
ro.product.device=mdarcy;features=sendrecv_v2_brotli,remount_shell,sendrecv_v2,abb,\
fixed_push_mkdir,fixed_push_symlink_timestamp,abb_exec,cmd,shell_v2,stat_v2,ls_v2,apex,delayed_ack";

    #[test]
    fn host_banner_lists_only_implemented_features_without_a_nul() {
        let banner = host_banner();
        assert_eq!(
            banner,
            b"host::features=shell_v2,cmd,stat_v2,ls_v2,sendrecv_v2,abb_exec"
        );
        assert!(!banner.contains(&0));
        assert!(banner.len() <= MAX_PAYLOAD_V1);
        assert!(!HOST_FEATURES.contains(&"delayed_ack"));
    }

    #[test]
    fn modern_banner_negotiates_the_feature_intersection() {
        let info = ConnectionInfo::negotiate(A_VERSION_SKIP_CHECKSUM, MAX_PAYLOAD, MODERN).unwrap();
        let mut features: Vec<_> = info.features().iter().map(String::as_str).collect();
        features.sort_unstable();
        assert_eq!(
            features,
            [
                "abb_exec",
                "cmd",
                "ls_v2",
                "sendrecv_v2",
                "shell_v2",
                "stat_v2"
            ]
        );
        assert!(info.has_feature("sendrecv_v2"));
        // Device-only flags are visible but never count as negotiated.
        assert!(info.device_features().contains("fixed_push_mkdir"));
        assert!(!info.has_feature("fixed_push_mkdir"));
        assert!(!info.has_feature("delayed_ack"));
        assert_eq!(info.system_type(), "device");
        assert_eq!(info.property("ro.product.model"), Some("SHIELD Android TV"));
    }

    #[test]
    fn version_is_the_minimum_so_checksums_stay_on() {
        let info = ConnectionInfo::negotiate(A_VERSION_SKIP_CHECKSUM, MAX_PAYLOAD, MODERN).unwrap();
        assert_eq!(info.protocol_version(), A_VERSION_MIN);
        assert!(info.verifies_checksums());
    }

    #[test]
    fn max_payload_is_the_minimum_of_both_sides() {
        let info = ConnectionInfo::negotiate(A_VERSION_MIN, 256 * 1024, MODERN).unwrap();
        assert_eq!(info.max_payload(), 256 * 1024);
        let info = ConnectionInfo::negotiate(A_VERSION_MIN, 4096, MODERN).unwrap();
        assert_eq!(info.max_payload(), 4096);
        let info = ConnectionInfo::negotiate(A_VERSION_MIN, u32::MAX, MODERN).unwrap();
        assert_eq!(info.max_payload(), MAX_PAYLOAD as usize);
    }

    #[test]
    fn zero_max_payload_is_rejected() {
        let error = ConnectionInfo::negotiate(A_VERSION_MIN, 0, MODERN).unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn legacy_banners_have_no_features() {
        for banner in [
            &b"device::ro.product.name=foo;ro.product.model=bar;ro.product.device=baz;\0"[..],
            b"device::\0",
            b"device",
            b"",
            b"device::features=",
        ] {
            let info = ConnectionInfo::negotiate(A_VERSION_MIN, 4096, banner).unwrap();
            assert!(info.features().is_empty(), "{banner:?}");
            assert!(info.device_features().is_empty(), "{banner:?}");
        }
        let info = ConnectionInfo::negotiate(
            A_VERSION_MIN,
            4096,
            b"device::ro.product.name=foo;ro.product.model=bar;\0",
        )
        .unwrap();
        assert_eq!(info.property("ro.product.model"), Some("bar"));
    }

    #[test]
    fn a_nul_terminated_feature_list_keeps_its_last_feature() {
        let info =
            ConnectionInfo::negotiate(A_VERSION_MIN, 4096, b"device::features=cmd,shell_v2\0")
                .unwrap();
        assert!(info.has_feature("shell_v2"));
        assert!(info.has_feature("cmd"));
    }

    #[test]
    fn a_colon_in_a_property_does_not_hide_the_features() {
        let info = ConnectionInfo::negotiate(
            A_VERSION_MIN,
            4096,
            b"device::ro.product.model=TV: 4K;features=shell_v2",
        )
        .unwrap();
        assert_eq!(info.property("ro.product.model"), Some("TV: 4K"));
        assert!(info.has_feature("shell_v2"));
    }

    #[test]
    fn unnegotiated_state_claims_nothing() {
        let info = ConnectionInfo::unnegotiated();
        assert!(info.features().is_empty());
        assert_eq!(info.max_payload(), MAX_PAYLOAD_V1);
        assert!(info.verifies_checksums());
    }
}
