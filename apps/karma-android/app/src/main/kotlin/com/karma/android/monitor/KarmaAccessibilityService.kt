package com.karma.android.monitor

import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.graphics.Bitmap
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import android.view.Display
import android.view.accessibility.AccessibilityEvent
import com.karma.android.R
import com.karma.android.guard.DeviceOwnerPolicies
import com.karma.android.guard.GuardState
import com.karma.android.guard.SettingsWatchdog
import com.karma.android.inference.ImageClassifier
import com.karma.android.monitor.context.BrowserUrlExtractor
import com.karma.android.monitor.context.TitleExtractor
import com.karma.android.monitor.enforce.Enforcer
import com.karma.android.security.CoreBootstrap
import com.karma.android.service.Alerts
import com.karma.android.service.MonitorForegroundService
import java.time.LocalDateTime
import java.util.concurrent.Executor
import uniffi.karma_mobile_ffi.AuditEventRecord
import uniffi.karma_mobile_ffi.ForegroundContext
import uniffi.karma_mobile_ffi.KarmaCore
import uniffi.karma_mobile_ffi.MobileAction
import uniffi.karma_mobile_ffi.MobileDecision
import uniffi.karma_mobile_ffi.PackageClass

/**
 * The monitor (technical plan §4.2–4.4). Tracks the foreground package, takes
 * screenshots at the pace [FramePacer] allows, scores them on a worker thread, asks the
 * Rust core for a decision and enforces it.
 *
 * Pixels never leave this class: each screenshot is copied once into a reused buffer,
 * reduced to the model tensor, and the buffers are cleared. Nothing is written to disk.
 */
class KarmaAccessibilityService : AccessibilityService() {
    private val main = Handler(Looper.getMainLooper())
    private val workerThread = HandlerThread("karma-monitor").apply { start() }
    private val worker = Handler(workerThread.looper)
    private val workerExecutor = Executor { worker.post(it) }
    private val pacer = FramePacer()

    private lateinit var core: KarmaCore
    private lateinit var enforcer: Enforcer
    private lateinit var watchdog: SettingsWatchdog
    @Volatile
    private var classifier: ImageClassifier? = null
    private var pixels = IntArray(0)

    private var foregroundPackage: String? = null
    private var foregroundClass = PackageClass.EXEMPT
    private var localExempt: Set<String> = emptySet()
    private var connected = false

    private val tick = object : Runnable {
        override fun run() {
            maybeCapture()
            main.postDelayed(this, FramePacer.TICK_MS)
        }
    }

    override fun onServiceConnected() {
        core = CoreBootstrap.get(this)
        enforcer = Enforcer(this, core)
        watchdog = SettingsWatchdog(this, core)
        localExempt = launcherAndKeyboardPackages()
        connected = true
        MonitorStatus.update { it.copy(serviceConnected = true) }
        MonitorForegroundService.start(this)
        runCatching { DeviceOwnerPolicies.apply(this) }
        enforcer.restoreSuspensions()
        audit("accessibility_connected", "observed")
        worker.post { loadClassifier() }
        main.post(tick)
    }

    private fun loadClassifier() {
        when (val result = ImageClassifier.load(this)) {
            is ImageClassifier.LoadResult.Ready -> {
                classifier = result.classifier
                MonitorStatus.update { it.copy(model = MonitorStatus.Model.READY, modelDetail = result.classifier.manifest.version) }
            }
            ImageClassifier.LoadResult.Missing ->
                MonitorStatus.update { it.copy(model = MonitorStatus.Model.MISSING, modelDetail = null) }
            is ImageClassifier.LoadResult.Rejected -> {
                MonitorStatus.update { it.copy(model = MonitorStatus.Model.REJECTED, modelDetail = result.reason) }
                audit("model_rejected", "rejected")
            }
        }
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        if (!connected || event == null) return
        val now = SystemClock.elapsedRealtime()
        val eventPackage = event.packageName?.toString() ?: return

        if (watchdog.isGuardedPackage(eventPackage)) {
            if (watchdog.inspect(eventPackage, rootInActiveWindow, now)) return
        }

        when (event.eventType) {
            AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED, AccessibilityEvent.TYPE_WINDOWS_CHANGED -> {
                val active = rootInActiveWindow?.packageName?.toString() ?: eventPackage
                if (active != foregroundPackage) onForegroundChanged(active, event, now)
                else pacer.onContentEvent(now)
            }
            AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED, AccessibilityEvent.TYPE_VIEW_SCROLLED ->
                if (eventPackage == foregroundPackage) pacer.onContentEvent(now)
        }
    }

    private fun onForegroundChanged(packageName: String, event: AccessibilityEvent, now: Long) {
        foregroundPackage = packageName
        foregroundClass = if (packageName in localExempt) PackageClass.EXEMPT else core.classifyPackage(packageName)
        pacer.onForeground(foregroundClass, now)
        if (foregroundClass == PackageClass.EXEMPT) return
        // Blocked apps, schedules, cooldowns and title rules apply before any frame.
        val title = TitleExtractor.title(this, event)
        worker.post {
            val decision = core.observeContext(context(packageName, title, host = null, secure = false))
            enforce(packageName, decision)
        }
    }

    private fun maybeCapture() {
        val packageName = foregroundPackage ?: return
        val now = SystemClock.elapsedRealtime()
        if (!pacer.shouldCapture(now)) return
        pacer.onCaptureStarted(now)
        val packageClass = foregroundClass
        if (classifier == null) {
            // No model: keep enforcing address-bar and title rules at the same pace.
            worker.post {
                try {
                    val decision = core.observeContext(currentContext(packageName, packageClass, secure = false))
                    enforce(packageName, decision)
                } finally {
                    main.post { pacer.onCaptureFinished() }
                }
            }
            return
        }
        takeScreenshot(Display.DEFAULT_DISPLAY, workerExecutor, object : TakeScreenshotCallback {
            override fun onSuccess(screenshot: ScreenshotResult) {
                try {
                    scoreFrame(packageName, packageClass, screenshot)
                } catch (error: Exception) {
                    audit("frame_failed", error.javaClass.simpleName.lowercase().take(48).ifEmpty { "error" })
                } finally {
                    main.post { pacer.onCaptureFinished() }
                }
            }

            override fun onFailure(errorCode: Int) {
                try {
                    if (errorCode == AccessibilityService.ERROR_TAKE_SCREENSHOT_SECURE_WINDOW) {
                        val decision = core.observeContext(currentContext(packageName, packageClass, secure = true))
                        enforce(packageName, decision)
                    }
                } finally {
                    main.post { pacer.onCaptureFinished() }
                }
            }
        })
    }

    /** Runs on the worker thread. */
    private fun scoreFrame(packageName: String, packageClass: PackageClass, screenshot: ScreenshotResult) {
        val buffer = screenshot.hardwareBuffer
        val width: Int
        val height: Int
        try {
            val hardware = Bitmap.wrapHardwareBuffer(buffer, screenshot.colorSpace) ?: return
            val software = hardware.copy(Bitmap.Config.ARGB_8888, false)
            hardware.recycle()
            width = software.width
            height = software.height
            if (pixels.size < width * height) pixels = IntArray(width * height)
            software.getPixels(pixels, 0, width, 0, 0, width, height)
            software.recycle()
        } finally {
            buffer.close()
        }
        val probabilities = try {
            classifier!!.classify(pixels, width, height)
        } finally {
            pixels.fill(0)
        }
        val decision = core.observeFrame(probabilities, currentContext(packageName, packageClass, secure = false))
        MonitorStatus.update { it.copy(framesScored = it.framesScored + 1) }
        enforce(packageName, decision)
    }

    /** Runs on the worker thread; switches to the main thread to act. */
    private fun enforce(packageName: String, decision: MobileDecision) {
        if (decision.action == MobileAction.ALLOW) return
        if (!decision.fromCooldown && decision.expiresAtMs != null) {
            MonitorStatus.update {
                it.copy(closures = it.closures + 1, lastClosureReason = decision.reason, lastClosureAtMs = System.currentTimeMillis())
            }
        }
        main.post {
            // The user may have left the app since the frame; the cooldown in the core
            // still closes it on its next appearance.
            if (packageName == foregroundPackage) enforcer.apply(packageName, decision)
        }
    }

    private fun currentContext(packageName: String, packageClass: PackageClass, secure: Boolean): ForegroundContext {
        val root = rootInActiveWindow
        val host = if (packageClass == PackageClass.BROWSER) BrowserUrlExtractor.host(packageName, root) else null
        return context(packageName, TitleExtractor.title(this, null), host, secure)
    }

    private fun context(packageName: String, title: String, host: String?, secure: Boolean): ForegroundContext {
        val now = LocalDateTime.now()
        val minuteOfWeek = (now.dayOfWeek.value - 1) * 1440 + now.hour * 60 + now.minute
        return ForegroundContext(
            packageName = packageName,
            windowTitle = title,
            browserHost = host,
            secureWindow = secure,
            capturedAtMs = System.currentTimeMillis(),
            minuteOfWeek = minuteOfWeek.toUInt(),
        )
    }

    /** The default launcher and input method are part of the system UI for capture purposes. */
    private fun launcherAndKeyboardPackages(): Set<String> {
        val launcher = packageManager.resolveActivity(
            Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME),
            0,
        )?.activityInfo?.packageName
        val keyboard = Settings.Secure.getString(contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
            ?.substringBefore('/')
        return setOfNotNull(launcher, keyboard, packageName)
    }

    private fun audit(kind: String, outcome: String) {
        if (!::core.isInitialized) return
        worker.post {
            runCatching { core.recordAudit(AuditEventRecord(System.currentTimeMillis(), kind, outcome, null, null, null)) }
        }
    }

    override fun onInterrupt() = Unit

    override fun onUnbind(intent: Intent?): Boolean {
        connected = false
        MonitorStatus.update { it.copy(serviceConnected = false) }
        main.removeCallbacks(tick)
        audit("accessibility_disconnected", "observed")
        if (GuardState.isArmed(this) && !GuardState.inMaintenance()) {
            Alerts.protectionWeakened(this, getString(R.string.alert_accessibility_disabled))
        }
        return super.onUnbind(intent)
    }

    override fun onDestroy() {
        main.removeCallbacksAndMessages(null)
        if (::enforcer.isInitialized) enforcer.hideOverlay()
        worker.post {
            classifier?.close()
            classifier = null
            workerThread.quitSafely()
        }
        super.onDestroy()
    }
}
