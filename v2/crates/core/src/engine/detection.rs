//! Device type detection — *one* canonical function (resolves the v1 duplicate
//! detection paths flagged in `docs/FEATURES.md` §13.1).
//!
//! Takes the union of inputs that the two v1 paths used (manufacturer + brand +
//! model + device codename) so we don't regress on edge cases either v1 path
//! handled.

use serde::{Deserialize, Serialize};

use super::types::DeviceProperties;

/// Detected device class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Shield,
    GoogleTv,
    Unknown,
}

impl DeviceType {
    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Shield => "Nvidia Shield",
            Self::GoogleTv => "Google TV",
            Self::Unknown => "Unknown",
        }
    }
}

/// What the device itself said about being a TV.
///
/// Deliberately three-valued. `DeviceType::Unknown` answers "which app list
/// applies", which is not the same question — a TV nobody has catalogued is
/// still a TV and must never be called "not an Android TV" (#120).
/// Unreadable means unknown, and unknown claims nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvEvidence {
    /// It reported the `tv` characteristic or the leanback feature.
    Tv,
    /// It reported being something else — a phone, tablet, watch, car head
    /// unit or emulator — or answered "no" to the leanback feature.
    NotTv,
    /// It said neither. The overwhelmingly likely case for an odd OEM box.
    #[default]
    Unknown,
}

/// Characteristics tokens that positively name a non-TV form factor. A device
/// carrying one of these said what it is; anything else it might say
/// (`nosdcard`, `emulator`-less vendor strings, …) says nothing at all.
const NON_TV_CHARACTERISTICS: [&str; 5] = ["phone", "tablet", "watch", "automotive", "emulator"];

fn characteristics_tokens(props: &DeviceProperties) -> impl Iterator<Item = &str> {
    props.characteristics.split(',').map(str::trim)
}

/// Does this device report itself as a TV? Either signal is sufficient: some
/// shipping boxes report a `ro.build.characteristics` that omits `tv` while
/// `pm has-feature android.software.leanback` answers `true`.
fn reports_tv(props: &DeviceProperties) -> bool {
    characteristics_tokens(props).any(|c| c.eq_ignore_ascii_case("tv"))
        || props.leanback == Some(true)
}

/// Pure: classify what the device claimed about its own form factor.
pub fn tv_evidence(props: &DeviceProperties) -> TvEvidence {
    if reports_tv(props) {
        return TvEvidence::Tv;
    }
    let said_otherwise = characteristics_tokens(props).any(|c| {
        NON_TV_CHARACTERISTICS
            .iter()
            .any(|n| c.eq_ignore_ascii_case(n))
    }) || props.leanback == Some(false);
    if said_otherwise {
        TvEvidence::NotTv
    } else {
        TvEvidence::Unknown
    }
}

/// Decide the device class from harvested properties. Mirrors v1's combined
/// detection rules (see `Get-DeviceType` and `Get-Devices` in v1) but
/// consolidated into one function.
pub fn detect_device_type(props: &DeviceProperties) -> DeviceType {
    let brand = props.brand.to_ascii_lowercase();
    let model = props.model.to_ascii_lowercase();
    let device = props.device_codename.to_ascii_lowercase();
    let manufacturer = props.manufacturer.to_ascii_lowercase();
    // `ro.build.characteristics` carries `tv` on Android TV / Google TV, and
    // `android.software.leanback` is the platform's own answer to the same
    // question. Either separates a TV from a phone or tablet sharing the same
    // brand — a Google Pixel is `brand == "google"` too, so brand alone is not
    // enough to call something a TV.
    let is_tv = reports_tv(props);

    // Shield: any signal from Nvidia or known Shield codenames. Shield boxes
    // are never phones, so these strong signals don't need the `tv` gate.
    if brand == "nvidia"
        || manufacturer == "nvidia"
        || model.contains("shield")
        || matches!(device.as_str(), "foster" | "darcy" | "mdarcy" | "sif")
    {
        return DeviceType::Shield;
    }

    // Google TV by strong product-specific signals: Onn (Walmart) boxes
    // (`ott_...`), Chromecast, and the newer Streamer codenames are TV-only
    // products, so they stand on their own.
    if brand == "onn"
        || model.contains("onn")
        || model.contains("chromecast")
        || model.contains("sabrina")
        || model.contains("boreal")
        || device.starts_with("ott_")
        || matches!(device.as_str(), "sabrina" | "boreal")
    {
        return DeviceType::GoogleTv;
    }

    // Google / Amlogic branding only means Google TV when the device actually
    // reports the `tv` characteristic — otherwise it's a phone or tablet (e.g.
    // a Google Pixel) that happens to share the brand.
    if is_tv && (brand == "google" || manufacturer == "google" || manufacturer == "amlogic") {
        return DeviceType::GoogleTv;
    }

    // Any other device that reports itself as a TV: classify as Google TV so it
    // gets the Android-TV app list rather than nothing.
    if is_tv {
        return DeviceType::GoogleTv;
    }

    DeviceType::Unknown
}

/// Map a Shield device codename to a friendly model string.
pub fn shield_friendly_model(device_codename: &str) -> String {
    match device_codename.to_ascii_lowercase().as_str() {
        "mdarcy" => "Shield TV Pro (2019)".to_string(),
        "sif" => "Shield TV (2019 Tube)".to_string(),
        "darcy" => "Shield TV (2017)".to_string(),
        "foster" => "Shield TV (2015)".to_string(),
        other => format!("Shield TV ({})", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn props(brand: &str, model: &str, device: &str, manufacturer: &str) -> DeviceProperties {
        DeviceProperties {
            brand: brand.to_string(),
            model: model.to_string(),
            device_codename: device.to_string(),
            manufacturer: manufacturer.to_string(),
            ..Default::default()
        }
    }

    fn props_ch(
        brand: &str,
        model: &str,
        device: &str,
        manufacturer: &str,
        characteristics: &str,
    ) -> DeviceProperties {
        DeviceProperties {
            characteristics: characteristics.to_string(),
            ..props(brand, model, device, manufacturer)
        }
    }

    #[test]
    fn detects_shield_by_brand() {
        assert_eq!(
            detect_device_type(&props("NVIDIA", "Shield Android TV", "mdarcy", "NVIDIA")),
            DeviceType::Shield
        );
    }

    #[test]
    fn detects_shield_by_codename_only() {
        // No brand/manufacturer info — detection should still fire from codename.
        assert_eq!(
            detect_device_type(&props("", "", "foster", "")),
            DeviceType::Shield
        );
    }

    #[test]
    fn detects_googletv_by_onn_brand() {
        assert_eq!(
            detect_device_type(&props("onn", "Onn 4K Pro", "ott_xxx", "Amlogic")),
            DeviceType::GoogleTv
        );
    }

    #[test]
    fn detects_googletv_chromecast() {
        assert_eq!(
            detect_device_type(&props("Google", "Chromecast", "sabrina", "Google")),
            DeviceType::GoogleTv
        );
    }

    #[test]
    fn detects_googletv_streamer() {
        assert_eq!(
            detect_device_type(&props("Google", "Google TV Streamer", "boreal", "Google")),
            DeviceType::GoogleTv
        );
    }

    #[test]
    fn google_branded_phone_is_not_google_tv() {
        // A Google Pixel phone shares brand/manufacturer "google" but reports
        // no `tv` characteristic — it must not be classified as Google TV.
        assert_eq!(
            detect_device_type(&props_ch("google", "Pixel 10 Pro", "blazer", "Google", "")),
            DeviceType::Unknown
        );
    }

    #[test]
    fn google_branded_tv_with_tv_characteristic_is_google_tv() {
        assert_eq!(
            detect_device_type(&props_ch("google", "Some TV", "generic", "Google", "tv")),
            DeviceType::GoogleTv
        );
    }

    #[test]
    fn generic_box_reporting_tv_characteristic_is_google_tv() {
        assert_eq!(
            detect_device_type(&props_ch("Generic", "TV Box", "rk3328", "Generic", "tv")),
            DeviceType::GoogleTv
        );
    }

    #[test]
    fn unknown_when_no_signals() {
        assert_eq!(
            detect_device_type(&props("Generic", "Generic TV Box", "rk3328", "Generic")),
            DeviceType::Unknown
        );
    }

    /// #120: a Xiaomi TV Box S reports `nosdcard` — not `tv` — in
    /// `ro.build.characteristics`. Reading that as "not an Android TV" locks
    /// the user out of their own box; the leanback feature is the second
    /// signal that settles it.
    #[test]
    fn leanback_alone_is_enough_to_call_it_a_tv() {
        let props = DeviceProperties {
            leanback: Some(true),
            ..props_ch("Xiaomi", "MiBOX4", "cezanne", "Xiaomi", "nosdcard")
        };

        assert_eq!(detect_device_type(&props), DeviceType::GoogleTv);
        assert_eq!(tv_evidence(&props), TvEvidence::Tv);
    }

    #[test]
    fn a_device_that_said_nothing_either_way_is_unknown() {
        // No `tv`, no non-TV form factor, no readable leanback answer. The
        // honest verdict is that we do not know — not that it is not a TV.
        let props = props_ch("Generic", "TV Box", "rk3328", "Generic", "");

        assert_eq!(detect_device_type(&props), DeviceType::Unknown);
        assert_eq!(tv_evidence(&props), TvEvidence::Unknown);
    }

    #[test]
    fn a_phone_says_so_and_we_believe_it() {
        let props = props_ch("google", "Pixel 10 Pro", "blazer", "Google", "phone");

        assert_eq!(detect_device_type(&props), DeviceType::Unknown);
        assert_eq!(tv_evidence(&props), TvEvidence::NotTv);
    }

    #[test]
    fn non_tv_tokens_are_matched_per_token_and_case_insensitively() {
        for token in ["Tablet", "watch", "AUTOMOTIVE", "emulator"] {
            let props = props_ch("x", "y", "z", "w", &format!("nosdcard,{token}"));
            assert_eq!(tv_evidence(&props), TvEvidence::NotTv, "{token}");
        }
        // A substring is not a token: `phonecall` does not make it a phone.
        assert_eq!(
            tv_evidence(&props_ch("x", "y", "z", "w", "phonecall")),
            TvEvidence::Unknown
        );
    }

    #[test]
    fn a_leanback_no_outweighs_nothing_else() {
        let props = DeviceProperties {
            leanback: Some(false),
            ..props_ch("Generic", "Box", "rk3328", "Generic", "nosdcard")
        };

        assert_eq!(tv_evidence(&props), TvEvidence::NotTv);
    }

    #[test]
    fn tv_characteristic_still_wins_over_a_leanback_no() {
        // Contradictory answers: it explicitly claimed `tv`. Believing the
        // positive claim keeps the tools reachable, which is the failure mode
        // that matters here.
        let props = DeviceProperties {
            leanback: Some(false),
            ..props_ch("Generic", "Box", "rk3328", "Generic", "tv")
        };

        assert_eq!(tv_evidence(&props), TvEvidence::Tv);
        assert_eq!(detect_device_type(&props), DeviceType::GoogleTv);
    }

    #[test]
    fn shield_friendly_model_known() {
        assert_eq!(shield_friendly_model("mdarcy"), "Shield TV Pro (2019)");
        assert_eq!(shield_friendly_model("MDARCY"), "Shield TV Pro (2019)");
        assert_eq!(shield_friendly_model("foster"), "Shield TV (2015)");
    }

    #[test]
    fn shield_friendly_model_unknown_passes_through() {
        assert_eq!(shield_friendly_model("xyz"), "Shield TV (xyz)");
    }
}
