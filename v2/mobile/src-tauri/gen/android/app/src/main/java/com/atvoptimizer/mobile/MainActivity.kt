package com.atvoptimizer.mobile

import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.WindowInsetsController
import android.webkit.WebView
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  private var webView: WebView? = null

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    this.webView = webView
  }

  // The manifest keeps a font size change from recreating the activity.
  // WebView reads the font scale into its text zoom only when it is created
  // (AwSettings.updateFontScaleLocked), so pass a new scale on here, with the
  // same percent it would have derived.
  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    updateStatusBarAppearance(newConfig)
    val settings = webView?.settings ?: return
    val zoom = (newConfig.fontScale * 100).toInt()
    if (settings.textZoom != zoom) settings.textZoom = zoom
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)

    // Android 15+ enforces edge-to-edge for this target SDK. Older Android
    // already fits the activity to the system bars, so padding it would
    // apply those insets twice.
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.VANILLA_ICE_CREAM) return
    updateStatusBarAppearance(resources.configuration)

    // Android WebView versions do not consistently expose system-bar/cutout
    // insets through CSS env(). Keep the entire web surface inside those bars.
    // Consume only those insets so WebView cannot apply them a second time;
    // keyboard insets and accessibility/text scaling remain untouched.
    val content = findViewById<View>(android.R.id.content)
    val protectedTypes = WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(protectedTypes)
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.Builder(insets)
        .setInsets(protectedTypes, Insets.NONE)
        .build()
    }
    ViewCompat.requestApplyInsets(content)
  }

  private fun updateStatusBarAppearance(config: Configuration) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.VANILLA_ICE_CREAM) return

    // The activity's DayNight theme supplies the background behind the status bar.
    val lightStatusBar = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
    val appearance = if (config.isNightModeActive) 0 else lightStatusBar
    window.insetsController?.setSystemBarsAppearance(appearance, lightStatusBar)
  }
}
