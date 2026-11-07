package com.karma.android.monitor.enforce

import androidx.annotation.StringRes
import com.karma.android.R
import uniffi.karma_mobile_ffi.MobileReason

/** User-facing text for each decision reason. Never includes the matched content. */
@StringRes
fun MobileReason.label(): Int = when (this) {
    MobileReason.DEFAULT_ALLOW -> R.string.reason_default_allow
    MobileReason.PROTECTION_DISABLED -> R.string.reason_protection_disabled
    MobileReason.EXEMPT_APPLICATION -> R.string.reason_exempt_application
    MobileReason.ALLOWED_APPLICATION -> R.string.reason_allowed_application
    MobileReason.ALLOWED_WEBSITE -> R.string.reason_allowed_website
    MobileReason.TIME_WINDOW_BLOCKED -> R.string.reason_time_window_blocked
    MobileReason.APPLICATION_BLOCKED -> R.string.reason_application_blocked
    MobileReason.BLOCKED_WEBSITE -> R.string.reason_blocked_website
    MobileReason.TITLE_KEYWORD -> R.string.reason_title_keyword
    MobileReason.SECURE_WINDOW -> R.string.reason_secure_window
    MobileReason.IMAGE_IMMEDIATE, MobileReason.IMAGE_REPEATED, MobileReason.OCR_IMAGE_COMBINED ->
        R.string.reason_image
    MobileReason.OCR_ONLY_WARNING -> R.string.reason_ocr_warning
    MobileReason.SOURCE_UNCERTAIN -> R.string.reason_source_uncertain
}
