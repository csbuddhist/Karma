package com.karma.android.monitor.context

import android.accessibilityservice.AccessibilityService
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo

/**
 * The title of the foreground window (activity, page or video title). It is matched
 * against title keywords in Rust and then discarded; it is never stored or audited.
 *
 * Browsers report only their own name ("Chrome") as the window title and in window
 * events, so the page title is read from the root node of the web content, which
 * Chromium-based browsers and WebViews label with the document title.
 */
object TitleExtractor {
    private const val MAX_TITLE_CHARS = 512
    private const val MAX_SCANNED_NODES = 400
    private const val WEB_VIEW_CLASS = "android.webkit.WebView"

    fun title(service: AccessibilityService, event: AccessibilityEvent?, root: AccessibilityNodeInfo? = null): String {
        val window = service.windows.firstOrNull {
            it.isActive && it.type == AccessibilityWindowInfo.TYPE_APPLICATION
        }
        val fromWindow = window?.title?.toString()
        val fromEvent = event
            ?.takeIf { it.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED }
            ?.text
            ?.joinToString(" ")
        val fromPage = root?.let(::pageTitle)
        val title = listOfNotNull(fromPage, fromWindow, fromEvent)
            .map(String::trim)
            .filter(String::isNotEmpty)
            .distinct()
            .joinToString(" ")
        return title.take(MAX_TITLE_CHARS)
    }

    /**
     * The document title on the first web-content root under [root], or `null`. The web
     * content itself (page text) is not descended into.
     */
    fun pageTitle(root: AccessibilityNodeInfo): String? {
        val queue = ArrayDeque<AccessibilityNodeInfo>()
        queue.add(root)
        var scanned = 0
        while (queue.isNotEmpty() && scanned < MAX_SCANNED_NODES) {
            val node = queue.removeFirst()
            scanned++
            if (node.className?.toString() == WEB_VIEW_CLASS) {
                val text = node.text?.toString()?.trim().orEmpty()
                if (text.isNotEmpty()) return text
                continue
            }
            for (index in 0 until node.childCount) node.getChild(index)?.let(queue::add)
        }
        return null
    }
}
