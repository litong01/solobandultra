# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.kts.

# Keep Rust JNI bridge — ScoreLib and ChoirLib have native method declarations that R8
# must not rename or strip; JNI function names encode the full class path.
-keep class com.solobandultra.app.ScoreLib { *; }
-keep class com.solobandultra.app.ChoirLib { *; }

# Keep the JavaScript interface class used by the WebView for seek-to-tap.
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}
