# Android dependency notice inventory — release preparation

## Resolved baseline (2026-11-03)

`universalReleaseRuntimeClasspath` has been resolved with Gradle 8.14.3 / Android Gradle Plugin
8.11.0 / JDK 21. The portable [`android-runtime-inventory.json`](android-runtime-inventory.json)
records 94 resolved modules, the exact selected artifact hashes, POM hashes and declared licenses.
Metadata-only BOMs and multiplatform redirects are distinguished from binary artifacts.
[`android-runtime-notices.md`](android-runtime-notices.md) retains 33 distinct upstream notice
texts from JAR/AAR files and the Google SDKs' embedded third-party-license indexes, preserving
distinct copyright notices. `scripts/gen-notices.sh` includes these in both shipped notice files.

The resolved selection includes Core/Core KTX 1.15.0 (rather than the plugin's declared 1.13.1),
Kotlin stdlib 2.1.0, Browser 1.8.0, Jackson 2.15.3 and Play Billing 9.1.0. Billing also resolves
Google DataTransport, Firebase *encoder* libraries and Google Play services, including the
location library. Library presence alone does not establish location collection: the merged
debug manifest declares no coarse, fine or background location permission. Inspect the final
release manifest and actual SDK behavior before submitting Data safety.

Reproduce after initializing the Android project through a Tauri build:

```sh
export ATVOPT_ANDROID_INVENTORY=/absolute/task/path/android-runtime-inventory.raw.json
cd src-tauri/gen/android
./gradlew :app:exportReleaseDependencyInventory \
  --init-script ../../../scripts/export-android-runtime.gradle --no-daemon --max-workers=1
cd ../../..
python3 scripts/render-android-notices.py "$ATVOPT_ANDROID_INVENTORY" \
  --parent-pom /absolute/task/path/guava-parent-26.0-android.pom
scripts/gen-notices.sh
```

The Guava parent POM is the published `com.google.guava:guava-parent:26.0-android` POM from Maven
Central; `listenablefuture:1.0` inherits its Apache-2.0 license. The raw Gradle export contains
local cache paths: keep it in task evidence, not Git. The generated inventory contains no local
paths. Keep `JAVA_HOME`, `ANDROID_HOME` and `NDK_HOME` set as described in `RELEASE.md`.
Re-resolve and compare for the final candidate and after dependency changes. This baseline does
not certify an unbuilt signed AAB or replace the phone/TV and purchase gates.

## Declaration reference

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
| ADB plugin | androidx.activity:activity:1.10.1 (added 2026-11-08 for the Save a copy picker callback; Apache-2.0, already resolved at 1.10.1 through the app's activity-ktx, so the resolved runtime set is unchanged) |
| Billing plugin | com.android.billingclient:billing:9.1.0 (Android Software Development Kit License, not Apache-2.0) |
| Billing plugin | androidx.core:core-ktx:1.13.1; org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2 |
| Tauri Android | androidx.core:core-ktx:1.7.0; androidx.appcompat:appcompat:1.6.0; Material:1.7.0 |
| Tauri Android | com.fasterxml.jackson.core:jackson-databind:2.15.3 |
| Opener | androidx.browser:browser:1.8.0; androidx.core:core-ktx:1.9.0; Jackson Databind:2.15.3 |

All listed libraries except the Play Billing Library declare Apache-2.0 licensing. The Play
Billing Library is distributed under the Android Software Development Kit License Agreement;
record its terms and any transitive Google artifacts it pulls in when the resolved graph is
captured. Official project sources:
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
Reconcile selected versions with this declaration inventory and regenerate notices. The resolved
baseline above supersedes the declaration-only gap; the final candidate still needs comparison
against it before distribution.

## Locked published-crate check

The cached `tauri-2.11.2.crate` archive SHA-256 is
`437404997acf375d85f1177afa7e11bb971f274ed6a7b83a2a3e339015f4cc28`, exactly matching
`v2/Cargo.lock`. Its archived `tauri-2.11.2/mobile/android/build.gradle.kts` SHA-256 is
`aab1b0ecb929ea70b33ad42d7df55c185a3485f9609ec998047232b45df76a4d`; line 43 declares
`com.fasterxml.jackson.core:jackson-databind:2.15.3`. The local extracted file is byte-identical.
It does NOT declare jackson-datatype-json-org. A declaration seen in a newer cached Tauri version
must not be attributed to this locked package. Re-check when the Tauri pin changes. This verifies
declarations, not Gradle transitive resolution or packaged contents.
