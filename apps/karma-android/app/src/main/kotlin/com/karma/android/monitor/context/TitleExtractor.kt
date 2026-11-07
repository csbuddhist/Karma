package com.karma.android.monitor.context

import android.accessibilityservice.AccessibilityService
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityWindowInfo

/**
 * The title of the foreground window (activity, page or video title). It is matched
 * against title keywords in Rust and then discarded; it is never stored or audited.
 */
object TitleExtractor {
    private const val MAX_TITLE_CHARS = 512

    fun title(service: AccessibilityService, event: AccessibilityEvent?): String {
        val window = service.windows.firstOrNull {
            it.isActive && it.type == AccessibilityWindowInfo.TYPE_APPLICATION
        }
        val fromWindow = window?.title?.toString()
        val fromEvent = event
            ?.takeIf { it.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED }
            ?.text
            ?.joinToString(" ")
        val title = listOfNotNull(fromWindow, fromEvent)
            .filter(String::isNotBlank)
            .distinct()
            .joinToString(" ")
        return title.take(MAX_TITLE_CHARS)
    }
}
