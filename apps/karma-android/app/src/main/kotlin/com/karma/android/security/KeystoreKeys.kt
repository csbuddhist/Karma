package com.karma.android.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The 32-byte key that seals the Rust core state. It is random, stored only wrapped by a
 * non-exportable Android Keystore AES key, and unwrapped into memory at startup.
 *
 * The Keystore key does not require an unlocked device, so the monitor keeps working on
 * the lock screen and right after boot (once the user has unlocked once).
 */
object KeystoreKeys {
    private const val KEYSTORE = "AndroidKeyStore"
    private const val WRAPPING_ALIAS = "karma.state.wrapping.v1"
    private const val WRAPPED_FILE = "state-key.v1.bin"
    private const val STATE_KEY_BYTES = 32
    private const val IV_BYTES = 12
    private const val TAG_BITS = 128

    @Synchronized
    fun stateKey(context: Context): ByteArray {
        val file = AtomicFile(File(context.noBackupFilesDir, WRAPPED_FILE))
        val wrapping = wrappingKey()
        if (file.baseFile.exists()) {
            val sealed = file.readFully()
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(
                Cipher.DECRYPT_MODE,
                wrapping,
                GCMParameterSpec(TAG_BITS, sealed, 0, IV_BYTES),
            )
            return cipher.doFinal(sealed, IV_BYTES, sealed.size - IV_BYTES)
        }
        val stateKey = ByteArray(STATE_KEY_BYTES).also(SecureRandom()::nextBytes)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, wrapping)
        val sealed = cipher.iv + cipher.doFinal(stateKey)
        val stream = file.startWrite()
        try {
            stream.write(sealed)
            file.finishWrite(stream)
        } catch (error: Throwable) {
            file.failWrite(stream)
            throw error
        }
        return stateKey
    }

    private fun wrappingKey(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(WRAPPING_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                WRAPPING_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .setUnlockedDeviceRequired(false)
                .build(),
        )
        return generator.generateKey()
    }
}
