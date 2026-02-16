plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Copy sheet music files from the repo root (single source of truth) into
// the build directory so they appear as assets/sheetmusic/*.musicxml at runtime.
val copySheetMusic = tasks.register<Copy>("copySheetMusic") {
    from("../../sheetmusic")
    into(layout.buildDirectory.dir("generated/sheetmusic-assets/sheetmusic"))
}

android {
    namespace = "com.solobandultra.app"
    compileSdk = 34

    sourceSets {
        getByName("main") {
            assets.srcDirs("src/main/assets", layout.buildDirectory.dir("generated/sheetmusic-assets"))
        }
    }

    defaultConfig {
        applicationId = "com.solobandultra.app"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }
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

    buildFeatures {
        compose = true
    }

    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.10"
    }

    androidResources {
        // Don't compress the SoundFont — it's a 31 MB binary blob that doesn't
        // benefit from AAPT2 compression, and reading it uncompressed is faster.
        noCompress += "sf2"
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

dependencies {
    // Core Android
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.7.0")
    implementation("androidx.activity:activity-compose:1.8.2")
    implementation("androidx.appcompat:appcompat:1.6.1")

    // Compose BOM
    implementation(platform("androidx.compose:compose-bom:2024.02.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")

    // Navigation
    implementation("androidx.navigation:navigation-compose:2.7.7")

    // Media/Audio
    implementation("androidx.media:media:1.7.0")

    // Kinde Authentication SDK
    implementation("com.kinde:android-sdk:1.5.0")
    implementation("com.squareup.retrofit2:retrofit:2.9.0")
    implementation("com.squareup.retrofit2:converter-gson:2.9.0")

    // Debug
    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}

// Ensure sheet music files are copied before assets are merged into the APK.
afterEvaluate {
    tasks.matching { it.name.startsWith("merge") && it.name.endsWith("Assets") }.configureEach {
        dependsOn(copySheetMusic)
    }
}
