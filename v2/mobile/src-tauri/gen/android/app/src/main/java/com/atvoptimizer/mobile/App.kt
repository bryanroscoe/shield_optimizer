package com.atvoptimizer.mobile

import android.app.Application
import io.github.muntashirakon.adb.PRNGFixes
import org.conscrypt.Conscrypt
import java.security.Security

class App : Application() {
  override fun onCreate() {
    super.onCreate()
    PRNGFixes.apply()
    if (Security.getProvider(Conscrypt.newProvider().name) == null) {
      Security.insertProviderAt(Conscrypt.newProvider(), 1)
    }
  }
}
