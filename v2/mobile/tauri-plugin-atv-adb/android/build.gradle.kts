plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.tauri.atvadb"
    compileSdk = 36

    defaultConfig {
        minSdk = 24
        consumerProguardFiles("proguard-rules.pro")
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
}

// Self-declared so the plugin resolves libadb-android even against a pristine
// (regenerated) app project that hasn't had jitpack added to its root.
repositories {
    google()
    mavenCentral()
    maven(url = "https://jitpack.io")
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    // Android-11+ wireless-debugging client (pairing-code TLS + shell streams).
    implementation("com.github.MuntashirAkon:libadb-android:3.1.1")
    // TLS 1.3 + SPAKE2 provider libadb-android relies on for pairing.
    implementation("org.conscrypt:conscrypt-android:2.5.3")
    // X.509 self-signed cert generation for the persisted ADB key.
    implementation("org.bouncycastle:bcpkix-jdk15to18:1.81")
    implementation(project(":tauri-android"))
}
