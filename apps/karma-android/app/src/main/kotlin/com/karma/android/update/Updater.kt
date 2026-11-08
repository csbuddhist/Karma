package com.karma.android.update

import android.content.Context
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import com.karma.android.BuildConfig
import com.karma.android.security.CoreBootstrap
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import javax.net.ssl.HttpsURLConnection
import uniffi.karma_mobile_ffi.AndroidRelease
import uniffi.karma_mobile_ffi.AuditEventRecord
import uniffi.karma_mobile_ffi.checksumForFile
import uniffi.karma_mobile_ffi.orderAndroidReleases
import uniffi.karma_mobile_ffi.releaseListUrl
import uniffi.karma_mobile_ffi.releaseSources
import uniffi.karma_mobile_ffi.selectAndroidRelease
import uniffi.karma_mobile_ffi.updateDownloadLimits

/**
 * Finds, downloads and verifies a newer Karma APK from GitHub or its Gitee mirror, then
 * hands it to [UpdateInstaller]. Which release and which URLs are acceptable is decided by
 * the Rust core; this class only performs HTTPS requests.
 *
 * A downloaded APK is installed only when its SHA-256 equals the release's checksum file,
 * its package name is Karma's, its version code is higher, and it is signed by exactly the
 * certificates of the installed app. Android enforces the signature again at install.
 * Nothing about the device or its use is sent; requests carry only a generic user agent.
 */
object Updater {
    enum class Outcome { UP_TO_DATE, INSTALLING, INSTALLED, NETWORK_FAILED, VERIFICATION_FAILED, INSTALL_BLOCKED }

    private const val CONNECT_TIMEOUT_MS = 20_000
    private const val READ_TIMEOUT_MS = 60_000
    private const val DIRECTORY = "updates"

    private val running = java.util.concurrent.atomic.AtomicBoolean(false)

    /**
     * Runs one check; call from a background thread. Returns `null` when another check is
     * already running (the periodic and the on-demand worker share one download file).
     */
    fun run(context: Context): Outcome? {
        if (!running.compareAndSet(false, true)) return null
        return try {
            check(context)
        } finally {
            running.set(false)
        }
    }

    private fun check(context: Context): Outcome {
        val limits = updateDownloadLimits()
        val installed = BuildConfig.VERSION_NAME
        var answered = false
        val found = mutableListOf<AndroidRelease>()
        for (source in releaseSources()) {
            val list = runCatching { get(releaseListUrl(source), limits.releaseListBytes.toLong()) }.getOrNull() ?: continue
            answered = true
            selectAndroidRelease(source, list, installed)?.let(found::add)
        }
        val outcome = when {
            !answered -> Outcome.NETWORK_FAILED
            found.isEmpty() -> Outcome.UP_TO_DATE
            else -> install(context, orderAndroidReleases(found))
        }
        UpdateSettings.recordCheck(context, outcome, found.firstOrNull()?.version)
        audit(context, "update_check", outcome.name.lowercase())
        return outcome
    }

    /** Tries each candidate (newest first, GitHub before Gitee) until one verifies. */
    private fun install(context: Context, candidates: List<AndroidRelease>): Outcome {
        if (!UpdateInstaller.canInstall(context)) return Outcome.INSTALL_BLOCKED
        val directory = File(context.noBackupFilesDir, DIRECTORY).apply { mkdirs() }
        clear(context)
        var outcome = Outcome.NETWORK_FAILED
        for (release in candidates) {
            val apk = File(directory, release.apkName)
            val verified = try {
                download(release, apk)
            } catch (_: IOException) {
                outcome = Outcome.NETWORK_FAILED
                apk.delete()
                continue
            }
            if (!verified || !isGenuineUpdate(context, apk)) {
                outcome = Outcome.VERIFICATION_FAILED
                apk.delete()
                continue
            }
            return if (UpdateInstaller.install(context, apk)) Outcome.INSTALLING else Outcome.INSTALL_BLOCKED
        }
        return outcome
    }

    /** Downloads the APK; returns whether its SHA-256 equals the published checksum. */
    private fun download(release: AndroidRelease, destination: File): Boolean {
        val limits = updateDownloadLimits()
        val checksums = get(release.checksumUrl, limits.checksumBytes.toLong()).decodeToString()
        val expected = checksumForFile(checksums, release.apkName) ?: return false
        val size = release.apkSize?.toLong()
        val limit = size ?: limits.apkBytes.toLong()
        val digest = MessageDigest.getInstance("SHA-256")
        val written = open(release.apkUrl).use { connection ->
            if (connection.contentLengthLong > limit) throw IOException("too large")
            connection.inputStream.use { input -> destination.outputStream().use { copy(input, it, limit, digest) } }
        }
        if (written == 0L || (size != null && written != size)) return false
        return digest.digest().joinToString("") { "%02x".format(it) } == expected
    }

    /** Same package, higher version code, identical signing certificates. */
    fun isGenuineUpdate(context: Context, apk: File): Boolean {
        val pm = context.packageManager
        val archive = pm.getPackageArchiveInfo(apk.path, PackageManager.GET_SIGNING_CERTIFICATES) ?: return false
        val installed = pm.getPackageInfo(context.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        if (archive.packageName != context.packageName || archive.longVersionCode <= installed.longVersionCode) return false
        val archiveSigners = signers(archive) ?: return false
        val installedSigners = signers(installed) ?: return false
        return archiveSigners.isNotEmpty() && archiveSigners == installedSigners
    }

    private fun signers(info: PackageInfo): Set<String>? {
        val signing = info.signingInfo ?: return null
        if (signing.hasMultipleSigners()) {
            return signing.apkContentsSigners.map { sha256(it.toByteArray()) }.toSet()
        }
        // A rotated key lists its lineage; the current signer is the last entry.
        return signing.signingCertificateHistory?.lastOrNull()?.let { setOf(sha256(it.toByteArray())) }
    }

    private fun sha256(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    /** Deletes downloaded APKs, for example after an update was installed. */
    fun clear(context: Context) {
        File(context.noBackupFilesDir, DIRECTORY).listFiles()?.forEach { it.delete() }
    }

    private fun get(url: String, limit: Long): ByteArray = open(url).use { connection ->
        if (connection.contentLengthLong > limit) throw IOException("too large")
        connection.inputStream.use { input ->
            val output = java.io.ByteArrayOutputStream()
            copy(input, output, limit, null)
            output.toByteArray()
        }
    }

    private fun open(url: String): AutoCloseableConnection {
        val connection = URL(url).openConnection() as? HttpsURLConnection ?: throw IOException("not https")
        connection.connectTimeout = CONNECT_TIMEOUT_MS
        connection.readTimeout = READ_TIMEOUT_MS
        // Release downloads redirect to the hosts' storage; Java never follows HTTPS → HTTP.
        connection.instanceFollowRedirects = true
        connection.setRequestProperty("User-Agent", "Karma-Android-Updater/${BuildConfig.VERSION_NAME}")
        if (url.startsWith("https://api.github.com/")) {
            connection.setRequestProperty("Accept", "application/vnd.github+json")
        }
        if (connection.responseCode != HttpURLConnection.HTTP_OK) {
            connection.disconnect()
            throw IOException("HTTP ${connection.responseCode}")
        }
        if (connection.url.protocol != "https") {
            connection.disconnect()
            throw IOException("redirected off HTTPS")
        }
        return AutoCloseableConnection(connection)
    }

    private class AutoCloseableConnection(val connection: HttpsURLConnection) : AutoCloseable {
        val contentLengthLong get() = connection.contentLengthLong
        val inputStream: InputStream get() = connection.inputStream
        override fun close() = connection.disconnect()
    }

    private fun copy(input: InputStream, output: OutputStream, limit: Long, digest: MessageDigest?): Long {
        val buffer = ByteArray(64 * 1024)
        var total = 0L
        while (true) {
            val read = input.read(buffer)
            if (read < 0) return total
            total += read
            if (total > limit) throw IOException("too large")
            digest?.update(buffer, 0, read)
            output.write(buffer, 0, read)
        }
    }

    fun audit(context: Context, kind: String, outcome: String) {
        runCatching {
            CoreBootstrap.get(context).recordAudit(AuditEventRecord(System.currentTimeMillis(), kind, outcome, null, null, null))
        }
    }
}
