package com.solobandultra.app.ui

import android.content.res.Configuration
import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext
import java.util.Locale

/**
 * CompositionLocal for the app's selected language tag (e.g. "en", "zh-Hans").
 * Empty string = use system default. Updated when user taps Apply in Settings;
 * no activity recreate, so the score/music does not re-render.
 */
val LocalAppLocale = staticCompositionLocalOf { "" }

/**
 * Returns the string for [stringResId] in the given [localeTag].
 * Uses a configuration context only for this lookup (never replaces LocalContext),
 * so the rest of the app (theme, score view) is unchanged and music does not re-render.
 *
 * @param stringResId String resource id (e.g. R.string.menu_settings).
 * @param localeTag Language tag (e.g. "zh-Hans") or empty for system default.
 *                  When null, uses [LocalAppLocale].current.
 */
@Composable
fun stringResourceForLocale(
    @StringRes stringResId: Int,
    localeTag: String? = null
): String {
    val context = LocalContext.current
    val tag = localeTag ?: LocalAppLocale.current
    if (tag.isEmpty()) {
        return context.getString(stringResId)
    }
    return try {
        val config = Configuration(context.resources.configuration)
        config.setLocale(Locale.forLanguageTag(tag))
        val localeContext = context.createConfigurationContext(config)
        localeContext.getString(stringResId)
    } catch (e: Exception) {
        context.getString(stringResId)
    }
}
