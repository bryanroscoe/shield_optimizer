plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.tauri.atvbilling"
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

repositories {
    google()
    mavenCentral()
}

dependencies {
    // Play Billing Library. Play requires 8 or newer for new apps and updates
    // from 2026-08-31; 9.x needs androidx.core 1.9+, which core-ktx below
    // satisfies. Its manifest contributes com.android.vending.BILLING.
    implementation("com.android.billingclient:billing:9.1.0")
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation(project(":tauri-android"))
}
