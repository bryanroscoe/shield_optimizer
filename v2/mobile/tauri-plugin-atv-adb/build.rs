// This plugin exposes no JS-invokable commands — it is a Rust-side transport
// used by the app's WirelessAdb driver. Its only job on Android is to ship the
// libadb-android-backed Kotlin plugin (android/) into the app build.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
