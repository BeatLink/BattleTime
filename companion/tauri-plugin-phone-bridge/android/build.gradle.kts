import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.neotime.companion.phonebridge"
    compileSdk = 36

    defaultConfig {
        // btleplug's Java needs Android 7.
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }

    // The Expo module's services and btleplug's Java, which build.rs copies here.
    sourceSets["main"].java.srcDir(".generated/java")
    sourceSets["main"].res.srcDir(".generated/res")

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.9.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation(project(":tauri-android"))
}
