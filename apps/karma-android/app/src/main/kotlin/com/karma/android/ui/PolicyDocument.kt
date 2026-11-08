package com.karma.android.ui

import org.json.JSONArray
import org.json.JSONObject

/**
 * The subset of the shared policy document (same shape as the desktop console's) that
 * the phone UI edits. Unknown keys are preserved, so a policy imported from the desktop
 * keeps its keyword, schedule and category settings.
 */
class PolicyDocument(json: String) {
    private val root: JSONObject = runCatching { JSONObject(json) }.getOrDefault(JSONObject())

    data class AppRule(val packageName: String, val label: String, val block: Boolean)
    data class SiteRule(val host: String, val block: Boolean)

    /** A daily `HH:MM` window; an end not after the start continues past midnight. */
    data class TimeWindow(val start: String, val end: String)

    /** [allow]: the app may be used only inside [windows]; otherwise it is closed inside them. */
    data class AppSchedule(val packageName: String, val label: String, val allow: Boolean, val windows: List<TimeWindow>)

    var protectionEnabled: Boolean
        get() = root.optBoolean("protectionEnabled", true)
        set(value) {
            root.put("protectionEnabled", value)
        }

    var imageEnabled: Boolean
        get() = root.optJSONObject("recognition")?.optBoolean("imageEnabled", true) ?: true
        set(value) {
            section("recognition").put("imageEnabled", value)
        }

    var closeSecureBrowserWindows: Boolean
        get() = root.optJSONObject("android")?.optString("secureWindowBrowserAction", "close") != "allow"
        set(value) {
            section("android").put("secureWindowBrowserAction", if (value) "close" else "allow")
        }

    var cooldownSeconds: Int
        get() = root.optJSONObject("android")?.optInt("cooldownSeconds", DEFAULT_COOLDOWN) ?: DEFAULT_COOLDOWN
        set(value) {
            section("android").put("cooldownSeconds", value.coerceIn(MIN_COOLDOWN, MAX_COOLDOWN))
        }

    /** Enabled allow/block rules created on the phone or the desktop. */
    val appRules: List<AppRule>
        get() = objects("applications")
            .filter { it.optBoolean("enabled") && it.optString("action") in setOf("allow", "block") }
            .map { AppRule(it.optString("executable"), it.optString("name"), it.optString("action") == "block") }

    fun setAppRule(packageName: String, label: String, block: Boolean) {
        removeAppRule(packageName)
        removeAppSchedule(packageName)
        array("applications").put(
            JSONObject()
                .put("id", "android:$packageName")
                .put("name", label.take(128).ifBlank { packageName })
                .put("executable", packageName)
                .put("category", "custom")
                .put("action", if (block) "block" else "allow")
                .put("enabled", true),
        )
    }

    fun removeAppRule(packageName: String) = removeWhere("applications") {
        it.optString("executable").equals(packageName, ignoreCase = true) && it.optString("action") in setOf("allow", "block")
    }

    /** Daily time windows per app (`android.appSchedules`), enforced by the Rust core. */
    val appSchedules: List<AppSchedule>
        get() {
            val array = root.optJSONObject("android")?.optJSONArray("appSchedules") ?: return emptyList()
            return (0 until array.length()).mapNotNull { array.optJSONObject(it) }.map { rule ->
                val windows = rule.optJSONArray("windows")
                AppSchedule(
                    rule.optString("package"),
                    rule.optString("name"),
                    rule.optString("mode") == "allow",
                    (0 until (windows?.length() ?: 0)).mapNotNull { windows?.optJSONObject(it) }
                        .map { TimeWindow(it.optString("start"), it.optString("end")) },
                )
            }
        }

    /** Replaces any rule for the app: one app has either an always-on rule or a schedule. */
    fun setAppSchedule(schedule: AppSchedule) {
        removeAppRule(schedule.packageName)
        removeAppSchedule(schedule.packageName)
        val windows = JSONArray()
        schedule.windows.forEach { windows.put(JSONObject().put("start", it.start).put("end", it.end)) }
        val android = section("android")
        val rules = android.optJSONArray("appSchedules") ?: JSONArray().also { android.put("appSchedules", it) }
        rules.put(
            JSONObject()
                .put("package", schedule.packageName)
                .put("name", schedule.label.take(128).ifBlank { schedule.packageName })
                .put("mode", if (schedule.allow) "allow" else "block")
                .put("windows", windows),
        )
    }

    fun removeAppSchedule(packageName: String) {
        val array = root.optJSONObject("android")?.optJSONArray("appSchedules") ?: return
        val kept = JSONArray()
        for (index in 0 until array.length()) {
            val item = array.opt(index)
            if (!(item is JSONObject && item.optString("package").equals(packageName, ignoreCase = true))) kept.put(item)
        }
        section("android").put("appSchedules", kept)
    }

    val siteRules: List<SiteRule>
        get() = objects("websites")
            .filter { it.optBoolean("enabled") }
            .map { SiteRule(it.optString("pattern"), it.optString("action") == "block") }

    fun setSiteRule(host: String, block: Boolean) {
        val normalized = host.trim().lowercase().removePrefix("https://").removePrefix("http://").substringBefore('/')
        if (normalized.isEmpty()) return
        removeSiteRule(normalized)
        array("websites").put(
            JSONObject()
                .put("id", "android:${if (block) "block" else "allow"}:$normalized")
                .put("pattern", normalized)
                .put("action", if (block) "block" else "allow")
                .put("enabled", true),
        )
    }

    fun removeSiteRule(host: String) = removeWhere("websites") { it.optString("pattern").equals(host, ignoreCase = true) }

    fun toJson(): String = root.toString()

    private fun section(name: String): JSONObject =
        root.optJSONObject(name) ?: JSONObject().also { root.put(name, it) }

    private fun array(name: String): JSONArray =
        root.optJSONArray(name) ?: JSONArray().also { root.put(name, it) }

    private fun objects(name: String): List<JSONObject> {
        val array = root.optJSONArray(name) ?: return emptyList()
        return (0 until array.length()).mapNotNull { array.optJSONObject(it) }
    }

    private fun removeWhere(name: String, predicate: (JSONObject) -> Boolean) {
        val array = root.optJSONArray(name) ?: return
        val kept = JSONArray()
        for (index in 0 until array.length()) {
            val item = array.opt(index)
            if (!(item is JSONObject && predicate(item))) kept.put(item)
        }
        root.put(name, kept)
    }

    companion object {
        const val DEFAULT_COOLDOWN = 60
        const val MIN_COOLDOWN = 10
        const val MAX_COOLDOWN = 3600
        /** Limits the Rust core enforces for `appSchedules`. */
        const val MAX_APP_SCHEDULES = 64
        const val MAX_WINDOWS = 8
    }
}
