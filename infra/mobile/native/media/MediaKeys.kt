package org.openvietgis.secure

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Native Rust JNI only. Never a Flutter MethodChannel key service. */
class MediaKeys(context: Context) {
    private val root = File(context.applicationContext.noBackupFilesDir, "ovg-media-keys-v1")
    private val alias = "org.openvietgis.media.wrap.v1"
    private val lock = Any()
    private external fun nativeInstall(): Boolean
    fun install(): Boolean = nativeInstall()
    private fun master(create: Boolean): SecretKey? {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val existing = ks.getKey(alias, null)
        if (existing != null) return existing as? SecretKey
        if (!create || root.listFiles()?.isNotEmpty() == true) return null
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(KeyGenParameterSpec.Builder(alias,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256).setRandomizedEncryptionRequired(true).build())
            generateKey()
        }
    }
    // JNI caller wipes the returned byte array immediately after copying.
    fun getScopedKey(scope: String, create: Boolean): ByteArray? = synchronized(lock) {
        try {
            val u = UUID.fromString(scope)
            if (u.toString() != scope || u == UUID(0L, 0L)) return@synchronized null
            if (!root.exists() && (!create || !root.mkdirs())) return@synchronized null
            val path = File(root, "$scope.wrap")
            val atomic = AtomicFile(path)
            val exists = path.exists() || File(path.path + ".bak").exists()
            val key = master(create && !exists) ?: return@synchronized null
            val aad = "OVGKEY1:$scope".toByteArray(Charsets.UTF_8)
            if (exists) {
                val bytes = atomic.openRead().use { input ->
                    val b = ByteArray(61)
                    var n = 0
                    while (n < b.size) {
                        val read = input.read(b, n, b.size - n)
                        if (read < 0) break
                        n += read
                    }
                    if (n != 60) return@synchronized null
                    b.copyOf(60)
                }
                val c = Cipher.getInstance("AES/GCM/NoPadding")
                c.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, bytes, 0, 12))
                c.updateAAD(aad)
                val raw = c.doFinal(bytes, 12, 48)
                if (raw.size != 32) { raw.fill(0); return@synchronized null }
                return@synchronized raw
            }
            if (!create) return@synchronized null
            val raw = ByteArray(32).also { SecureRandom().nextBytes(it) }
            try {
                val c = Cipher.getInstance("AES/GCM/NoPadding")
                c.init(Cipher.ENCRYPT_MODE, key)
                c.updateAAD(aad)
                val encrypted = c.doFinal(raw)
                if (c.iv.size != 12 || encrypted.size != 48) return@synchronized null
                val out = atomic.startWrite()
                try {
                    out.write(c.iv); out.write(encrypted); atomic.finishWrite(out)
                } catch (e: Exception) {
                    atomic.failWrite(out); throw e
                }
                raw.copyOf()
            } finally { raw.fill(0) }
        } catch (_: Exception) {
            // No exception detail or key material to logs; never replace invalid keys.
            null
        }
    }
}
