# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.kts.

# Keep Rust JNI bindings (for future Rust integration)
-keep class com.solobandultra.app.rust.** { *; }
