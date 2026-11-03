// No JS-invokable commands: the app's billing commands call this plugin from
// Rust, and Rust (never the WebView) decides whether a purchase is genuine.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
