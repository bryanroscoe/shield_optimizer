package com.atvoptimizer.mobile

import android.content.res.Configuration
import android.graphics.Color
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
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.VANILLA_ICE_CREAM) {
      findViewById<View>(android.R.id.content).requestApplyInsets()
    }
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

    // Android WebView versions do not consistently expose system-bar/cutout
    // insets through CSS env(). Keep the entire web surface inside those bars.
    // Consume only those insets so WebView cannot apply them a second time;
    // keyboard insets and accessibility/text scaling remain untouched.
    val content = findViewById<View>(android.R.id.content)
    val protectedTypes = WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      // The bars are transparent. Draw their background under the padded
      // WebView and keep the icons readable after a system theme change.
      val dark = view.resources.configuration.isNightModeActive
      view.setBackgroundColor(if (dark) Color.BLACK else Color.WHITE)
      val lightBars = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
        WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
      view.windowInsetsController?.setSystemBarsAppearance(if (dark) 0 else lightBars, lightBars)

      val bars = insets.getInsets(protectedTypes)
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.Builder(insets)
        .setInsets(protectedTypes, Insets.NONE)
        .build()
    }
    ViewCompat.requestApplyInsets(content)
  }
}
