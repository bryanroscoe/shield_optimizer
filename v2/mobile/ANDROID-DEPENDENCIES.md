# Android dependency notice inventory — release preparation

Sources: app and ADB-plugin `build.gradle.kts`, plus the Android build files from locked
Tauri 2.11.2 and opener 2.5.4 Cargo registry packages. It is a declaration-level list, not a
resolved Gradle dependency report.

| Declaration site | Maven coordinates / declared version |
|---|---|
| App | androidx.webkit:webkit:1.14.0 |
| App | androidx.appcompat:appcompat:1.7.1 |
| App | androidx.activity:activity-ktx:1.10.1 |
| App | com.google.android.material:material:1.12.0 |
| App | androidx.lifecycle:lifecycle-process:2.10.0 |
| ADB plugin | androidx.core:core-ktx:1.13.1 |
| ADB plugin | org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2 |
| Tauri Android | androidx.core:core-ktx:1.7.0; androidx.appcompat:appcompat:1.6.0; Material:1.7.0 |
| Tauri Android | com.fasterxml.jackson.core:jackson-databind:2.15.3 |
| Opener | androidx.browser:browser:1.8.0; androidx.core:core-ktx:1.9.0; Jackson Databind:2.15.3 |

All listed libraries declare Apache-2.0 licensing. Official project sources:
https://android.googlesource.com/platform/frameworks/support/,
https://github.com/material-components/material-components-android,
https://github.com/Kotlin/kotlinx.coroutines,
https://github.com/FasterXML/jackson-databind,
https://github.com/FasterXML/jackson-datatypes-misc.
Do not confuse build plugins or test-only JUnit/Espresso with packaged runtime libraries.

Before distribution, initialize the Android project through the supported Tauri build flow,
then save Gradle `:app:dependencies --configuration universalReleaseRuntimeClasspath` (confirm
configuration name in that generated project). Inventory every resolved artifact, license and
upstream NOTICE, including transitive Kotlin/Jackson dependencies; preserve any required texts.
Reconcile selected versions with this declaration inventory and regenerate notices. This final
resolved-graph/license-text gate remains OPEN; it cannot be cleared by the declaration table.

## Locked published-crate check

The cached `tauri-2.11.2.crate` archive SHA-256 is
`437404997acf375d85f1177afa7e11bb971f274ed6a7b83a2a3e339015f4cc28`, exactly matching
`v2/Cargo.lock`. Its archived `tauri-2.11.2/mobile/android/build.gradle.kts` SHA-256 is
`aab1b0ecb929ea70b33ad42d7df55c185a3485f9609ec998047232b45df76a4d`; line 43 declares
`com.fasterxml.jackson.core:jackson-databind:2.15.3`. The local extracted file is byte-identical.
It does NOT declare jackson-datatype-json-org. A declaration seen in a newer cached Tauri version
must not be attributed to this locked package. Re-check when the Tauri pin changes. This verifies
declarations, not Gradle transitive resolution or packaged contents.
