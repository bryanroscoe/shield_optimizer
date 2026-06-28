package com.atvoptimizer.mobile

import android.app.Application

// Security-provider setup (Conscrypt + libadb PRNG fixes) lives in the
// tauri-plugin-atv-adb plugin's load(), so this stays a plain Application and
// the app module carries none of the transport's native dependencies.
class App : Application()
