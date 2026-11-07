package com.karma.android.monitor.context

import android.view.accessibility.AccessibilityNodeInfo
import java.net.URI

/**
 * Reads the host shown in a browser's address bar from the accessibility tree. Only the
 * host leaves this class; paths, queries and page text are never kept.
 */
object BrowserUrlExtractor {
    /** Address-bar view ids of browsers whose layout is known. */
    private val URL_BAR_IDS: Map<String, List<String>> = mapOf(
        "com.android.chrome" to listOf("com.android.chrome:id/url_bar"),
        "com.chrome.beta" to listOf("com.chrome.beta:id/url_bar"),
        "com.chrome.dev" to listOf("com.chrome.dev:id/url_bar"),
        "com.chrome.canary" to listOf("com.chrome.canary:id/url_bar"),
        "org.chromium.chrome" to listOf("org.chromium.chrome:id/url_bar"),
        "com.brave.browser" to listOf("com.brave.browser:id/url_bar"),
        "com.kiwibrowser.browser" to listOf("com.kiwibrowser.browser:id/url_bar"),
        "com.vivaldi.browser" to listOf("com.vivaldi.browser:id/url_bar"),
        "com.microsoft.emmx" to listOf("com.microsoft.emmx:id/url_bar"),
        "com.opera.browser" to listOf("com.opera.browser:id/url_field"),
        "com.opera.gx" to listOf("com.opera.gx:id/url_field"),
        "org.mozilla.firefox" to listOf("org.mozilla.firefox:id/mozac_browser_toolbar_url_view"),
        "org.mozilla.firefox_beta" to listOf("org.mozilla.firefox_beta:id/mozac_browser_toolbar_url_view"),
        "org.mozilla.fenix" to listOf("org.mozilla.fenix:id/mozac_browser_toolbar_url_view"),
        "org.mozilla.focus" to listOf("org.mozilla.focus:id/display_url", "org.mozilla.focus:id/mozac_browser_toolbar_url_view"),
        "com.sec.android.app.sbrowser" to listOf(
            "com.sec.android.app.sbrowser:id/location_bar_edit_text",
            "com.sec.android.app.sbrowser:id/custom_tab_toolbar_url_bar_text",
        ),
        "com.duckduckgo.mobile.android" to listOf("com.duckduckgo.mobile.android:id/omnibarTextInput"),
        "com.yandex.browser" to listOf("com.yandex.browser:id/bro_omnibar_address_title_text"),
        "com.mi.globalbrowser" to listOf("com.mi.globalbrowser:id/url"),
        "com.android.browser" to listOf("com.android.browser:id/url", "com.android.browser:id/title"),
        "com.heytap.browser" to listOf("com.heytap.browser:id/url_text"),
        "com.huawei.browser" to listOf("com.huawei.browser:id/url_bar", "com.huawei.browser:id/addressbar_url"),
        "com.vivo.browser" to listOf("com.vivo.browser:id/search_text"),
        "mark.via" to listOf("mark.via:id/search_text"),
        "mark.via.gp" to listOf("mark.via.gp:id/search_text"),
    )

    private const val MAX_SCANNED_NODES = 400

    fun host(packageName: String, root: AccessibilityNodeInfo?): String? {
        root ?: return null
        URL_BAR_IDS[packageName]?.forEach { id ->
            root.findAccessibilityNodeInfosByViewId(id).firstOrNull()?.text?.let { text ->
                hostFromAddressBar(text.toString())?.let { return it }
            }
        }
        // Unknown layout: the first editable or URL-like text near the top of the window.
        return scanForAddress(root)
    }

    private fun scanForAddress(root: AccessibilityNodeInfo): String? {
        val queue = ArrayDeque<AccessibilityNodeInfo>()
        queue.add(root)
        var scanned = 0
        while (queue.isNotEmpty() && scanned < MAX_SCANNED_NODES) {
            val node = queue.removeFirst()
            scanned++
            val text = node.text?.toString()
            if (text != null && (node.isEditable || node.viewIdResourceName?.contains("url", true) == true)) {
                hostFromAddressBar(text)?.let { return it }
            }
            for (index in 0 until node.childCount) node.getChild(index)?.let(queue::add)
        }
        return null
    }

    private val HOST_CHARS = Regex("^[a-z0-9.-]+$")

    /**
     * The lowercase host of an address-bar text such as `example.com/path` or
     * `https://m.example.com`, or `null` for search terms and non-web schemes.
     */
    fun hostFromAddressBar(text: String): String? {
        val trimmed = text.trim()
        if (trimmed.isEmpty() || trimmed.length > 2048 || trimmed.any(Char::isWhitespace)) return null
        val withScheme = if ("://" in trimmed) trimmed else "https://$trimmed"
        val uri = try {
            URI(withScheme)
        } catch (_: Exception) {
            return null
        }
        if (uri.scheme?.lowercase() !in setOf("http", "https")) return null
        val host = uri.host?.lowercase()?.trimEnd('.') ?: return null
        if ('.' !in host || !HOST_CHARS.matches(host)) return null
        return host
    }
}
