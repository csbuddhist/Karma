package com.karma.android.monitor.enforce

import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.graphics.Color
import android.graphics.PixelFormat
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import com.karma.android.R
import com.karma.android.ui.UnlockActivity
import uniffi.karma_mobile_ffi.MobileDecision

/**
 * Full-screen accessibility overlay shown after a closure: the reason, the time left in
 * the cooldown, and the administrator unlock entry. It covers whatever is on screen
 * while the app is sent home, so nothing more of the blocked content is visible.
 */
class BlockOverlay(private val service: AccessibilityService) {
    private val windowManager = service.getSystemService(WindowManager::class.java)
    private val handler = Handler(Looper.getMainLooper())
    private var view: View? = null
    private var countdown: TextView? = null
    private var expiresAtMs: Long? = null

    private val tick = object : Runnable {
        override fun run() {
            val remaining = (expiresAtMs ?: 0L) - System.currentTimeMillis()
            countdown?.text = if (remaining > 0) {
                val seconds = (remaining + 999) / 1000
                service.getString(R.string.overlay_cooldown, seconds / 60, seconds % 60)
            } else {
                ""
            }
            handler.postDelayed(this, 1000)
        }
    }

    fun show(packageName: String, decision: MobileDecision) {
        handler.post {
            hideNow()
            expiresAtMs = decision.expiresAtMs
            val layout = LinearLayout(service).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER
                setBackgroundColor(Color.rgb(18, 18, 20))
                setPadding(64, 64, 64, 64)
                isClickable = true
            }
            fun text(value: String, size: Float, color: Int) = TextView(service).apply {
                this.text = value
                textSize = size
                setTextColor(color)
                gravity = Gravity.CENTER
                setPadding(0, 16, 0, 16)
            }
            layout.addView(text(service.getString(R.string.overlay_title), 24f, Color.WHITE))
            layout.addView(text(service.getString(decision.reason.label()), 16f, Color.rgb(220, 220, 225)))
            countdown = text("", 14f, Color.rgb(170, 170, 180)).also(layout::addView)
            layout.addView(Button(service).apply {
                text = service.getString(R.string.overlay_dismiss)
                setOnClickListener { hide() }
            })
            layout.addView(Button(service).apply {
                text = service.getString(R.string.overlay_unlock)
                setOnClickListener {
                    hide()
                    service.startActivity(
                        Intent(service, UnlockActivity::class.java)
                            .putExtra(UnlockActivity.EXTRA_PURPOSE, UnlockActivity.PURPOSE_OPEN_APP)
                            .putExtra(UnlockActivity.EXTRA_PACKAGE, packageName)
                            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                    )
                }
            })
            val params = WindowManager.LayoutParams(
                WindowManager.LayoutParams.MATCH_PARENT,
                WindowManager.LayoutParams.MATCH_PARENT,
                WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
                PixelFormat.OPAQUE,
            )
            runCatching { windowManager.addView(layout, params) }
                .onSuccess {
                    view = layout
                    handler.post(tick)
                    handler.postDelayed({ hideNow() }, AUTO_HIDE_MS)
                }
        }
    }

    fun hide() {
        handler.post { hideNow() }
    }

    private fun hideNow() {
        handler.removeCallbacksAndMessages(null)
        view?.let { runCatching { windowManager.removeView(it) } }
        view = null
        countdown = null
    }

    companion object {
        private const val AUTO_HIDE_MS = 8_000L
    }
}
