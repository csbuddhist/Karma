package com.karma.android

import android.app.LocaleManager
import android.content.Context
import android.content.res.Configuration
import android.os.Build
import android.os.LocaleList
import java.util.Locale

/**
 * The app's display language, chosen in Karma without the administrator password.
 *
 * Android 13+ keeps the choice as the system per-app language, which applies to every
 * component of the process and matches the system "App languages" page. Android 11–12
 * have no per-app language, so the choice is kept in Karma's preferences and applied by
 * [localized] to activities (on attach) and to strings shown by services.
 */
object AppLanguage {
    enum class Choice(val tag: String) {
        SYSTEM(""),
        ENGLISH("en"),
        CHINESE("zh-CN"),
        ;

        companion object {
            /** Maps a stored or system language tag to a choice; unknown tags follow the system. */
            fun fromTag(tag: String?): Choice {
                val language = tag?.trim()?.substringBefore(',')?.substringBefore('-')?.substringBefore('_')?.lowercase()
                return when (language) {
                    "en" -> ENGLISH
                    "zh" -> CHINESE
                    else -> SYSTEM
                }
            }
        }
    }

    /** Every language the app ships strings for, used to recognise Karma's labels in any of them. */
    val SUPPORTED: List<Choice> = listOf(Choice.ENGLISH, Choice.CHINESE)

    private const val PREFS = "karma.language"
    private const val KEY_TAG = "tag"

    private val perAppSupported get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU

    @Volatile
    private var legacyChoice: Choice? = null

    fun current(context: Context): Choice =
        if (perAppSupported) {
            Choice.fromTag(context.getSystemService(LocaleManager::class.java).applicationLocales.toLanguageTags())
        } else {
            legacyChoice ?: Choice.fromTag(prefs(context).getString(KEY_TAG, "")).also { legacyChoice = it }
        }

    /**
     * Stores [choice]. On Android 13+ the system recreates open activities itself; earlier
     * versions return true so the caller recreates its activity.
     */
    fun select(context: Context, choice: Choice): Boolean {
        if (perAppSupported) {
            context.getSystemService(LocaleManager::class.java).applicationLocales = LocaleList.forLanguageTags(choice.tag)
            return false
        }
        prefs(context).edit().putString(KEY_TAG, choice.tag).apply()
        legacyChoice = choice
        applyDefaultLocale(context)
        return true
    }

    /** Makes date and number formatting follow the chosen language on Android 11–12. */
    fun applyDefaultLocale(context: Context) {
        if (perAppSupported) return
        val choice = current(context)
        val locale = if (choice == Choice.SYSTEM) {
            android.content.res.Resources.getSystem().configuration.locales[0]
        } else {
            Locale.forLanguageTag(choice.tag)
        }
        Locale.setDefault(locale)
    }

    /** [context] with the chosen language applied; unchanged on Android 13+ or when following the system. */
    fun localized(context: Context): Context {
        if (perAppSupported) return context
        val choice = current(context)
        return if (choice == Choice.SYSTEM) context else context.inLanguage(choice)
    }

    /** [resId] in every supported language, e.g. to recognise Karma's label in system settings. */
    fun allTranslations(context: Context, resId: Int): Set<String> =
        SUPPORTED.mapTo(linkedSetOf()) { context.inLanguage(it).getString(resId) }

    private fun Context.inLanguage(choice: Choice): Context {
        val configuration = Configuration(resources.configuration)
        configuration.setLocales(LocaleList.forLanguageTags(choice.tag))
        return createConfigurationContext(configuration)
    }

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
