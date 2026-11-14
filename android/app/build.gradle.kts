plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// Release signing comes from the environment only. Without it the release
// build stays unsigned: it never falls back to the debug key.
val keystorePath: String? = System.getenv("LUMEN_KEYSTORE")?.takeIf { it.isNotBlank() }

android {
    namespace = "io.github.vaspyyy.lumen"
    // 37: Compose 1.12 (BOM 2026.09.00) and core 1.19 require it; the app still targets 36.
    compileSdk = 37

    defaultConfig {
        applicationId = "io.github.vaspyyy.lumen"
        minSdk = 30
        targetSdk = 36
        versionCode = 700
        versionName = "0.7.0"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("LUMEN_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("LUMEN_KEY_ALIAS")
                keyPassword = System.getenv("LUMEN_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = if (keystorePath != null) signingConfigs.getByName("release") else null
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs.useLegacyPackaging = false
    }

    lint {
        abortOnError = true
        warningsAsErrors = false
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.androidx.biometric)
    implementation(libs.androidx.fragment)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.graphics)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.camera.camera2)
    implementation(libs.camera.lifecycle)
    implementation(libs.camera.view)
    implementation(libs.zxing.core)
}
