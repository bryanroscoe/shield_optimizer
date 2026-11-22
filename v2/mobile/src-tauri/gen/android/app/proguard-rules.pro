# Tauri and Wry generate the other JNI keep rules during the Rust build.
# Wry calls these methods from Rust by name, but its generated rules omit them.
-keepclassmembers class com.atvoptimizer.mobile.RustWebView {
    public void clearAllBrowsingData();
    public java.lang.String getCookies(java.lang.String);
}

# Tauri discovers inherited commands by name on the unannotated base class.
-keepclassmembers class app.tauri.plugin.Plugin {
    @app.tauri.annotation.Command public <methods>;
}
