package com.atvoptimizer.mobile

import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

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
}
