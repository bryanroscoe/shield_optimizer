import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Release signing material is resolved from `gen/android/keystore.properties`
// first, then from the environment, so a workstation can keep the keystore out
// of the tree and CI can inject it as secrets. Both are gitignored/absent by
// default. Debug builds never need them. A release task without them fails
// (see the task-graph check below) instead of quietly emitting an unsigned
// artifact; ATVOPT_ALLOW_UNSIGNED_RELEASE=1 is the explicit local escape hatch.
val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

fun signingValue(propertyKeys: List<String>, envKey: String): String? =
    propertyKeys.firstNotNullOfOrNull { keystoreProperties.getProperty(it) }
        ?.takeIf { it.isNotBlank() }
        ?: System.getenv(envKey)?.takeIf { it.isNotBlank() }

val releaseStorePath = signingValue(listOf("storeFile", "path"), "ATVOPT_KEYSTORE_PATH")
val releaseStorePassword = signingValue(listOf("storePassword"), "ATVOPT_KEYSTORE_PASSWORD")
val releaseKeyAlias = signingValue(listOf("keyAlias"), "ATVOPT_KEYSTORE_ALIAS")
val releaseKeyPassword = signingValue(listOf("keyPassword"), "ATVOPT_KEYSTORE_KEY_PASSWORD")

val releaseKeystore = releaseStorePath?.let { path ->
    val resolved = file(path).takeIf { it.isAbsolute } ?: rootProject.file(path)
    if (resolved.exists()) {
        resolved
    } else {
        logger.warn("ATV Optimizer: keystore path '$path' does not exist.")
        null
    }
}

val releaseSigningReady = releaseKeystore != null &&
    releaseStorePassword != null &&
    releaseKeyAlias != null &&
    releaseKeyPassword != null

val allowUnsignedRelease = System.getenv("ATVOPT_ALLOW_UNSIGNED_RELEASE") == "1"
val playDistribution = System.getenv("ATVOPT_DISTRIBUTION")?.trim().equals("play", ignoreCase = true)

gradle.taskGraph.whenReady {
    val releaseTasks = allTasks.filter { task ->
        task.project == project &&
            task.name.endsWith("Release") &&
            (task.name.startsWith("assemble") || task.name.startsWith("bundle") || task.name.startsWith("package"))
    }
    if (releaseTasks.isEmpty()) return@whenReady
    if (!releaseSigningReady && !allowUnsignedRelease) {
        throw GradleException(
            "ATV Optimizer: release signing is not configured, so this release build would be UNSIGNED " +
                "and Play would reject it. Create gen/android/keystore.properties " +
                "(storeFile/storePassword/keyAlias/keyPassword) or set ATVOPT_KEYSTORE_PATH, " +
                "ATVOPT_KEYSTORE_PASSWORD, ATVOPT_KEYSTORE_ALIAS and ATVOPT_KEYSTORE_KEY_PASSWORD. " +
                "For a deliberate local unsigned build set ATVOPT_ALLOW_UNSIGNED_RELEASE=1. " +
                "See mobile/RELEASE.md."
        )
    }
    // An AAB is the Play upload format. Built without the Play flag it would
    // carry offline license-key entry and no purchase verification.
    val bundling = releaseTasks.any { it.name.startsWith("bundle") }
    if (bundling && !playDistribution && !allowUnsignedRelease) {
        throw GradleException(
            "ATV Optimizer: release AABs are for Google Play. Set ATVOPT_DISTRIBUTION=play and " +
                "ATVOPT_PLAY_LICENSE_KEY (see mobile/RELEASE.md), or build an APK for sideloading."
        )
    }
}

android {
    compileSdk = 36
    namespace = "com.atvoptimizer.mobile"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.atvoptimizer.mobile"
        minSdk = 24
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (releaseSigningReady) {
            create("release") {
                storeFile = releaseKeystore
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("release")
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")
