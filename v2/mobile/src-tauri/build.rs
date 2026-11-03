fn main() {
    println!("cargo:rerun-if-env-changed=ATVOPT_DISTRIBUTION");
    println!("cargo:rerun-if-env-changed=ATVOPT_PLAY_LICENSE_KEY");
    check_play_release_config();
    tauri_build::build()
}

/// A Play release without a usable license key would ship with purchases
/// disabled and key entry hidden: no way to get Pro at all. Fail the build
/// instead of producing that artifact.
fn check_play_release_config() {
    let play = std::env::var("ATVOPT_DISTRIBUTION")
        .map(|v| v.trim().eq_ignore_ascii_case("play"))
        .unwrap_or(false);
    let release = std::env::var("PROFILE")
        .map(|p| p == "release")
        .unwrap_or(false);
    if !(play && release) {
        return;
    }
    let key: String = std::env::var("ATVOPT_PLAY_LICENSE_KEY")
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let looks_like_key = key.len() >= 300
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='));
    if !looks_like_key {
        panic!(
            "ATVOPT_DISTRIBUTION=play release build needs ATVOPT_PLAY_LICENSE_KEY set to the \
             app's base64 RSA license key from Play Console (Monetize > Monetization setup). \
             See v2/mobile/RELEASE.md."
        );
    }
}
