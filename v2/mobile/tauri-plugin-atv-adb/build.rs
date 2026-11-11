// This plugin exposes no JS-invokable commands. Its job on Android is to ship
// the Kotlin side (android/) that the app's Rust code calls: network
// discovery and watching, and document pick, save and share.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
