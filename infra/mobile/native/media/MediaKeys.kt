package org.openvietgis.secure

import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.os.Looper
import android.view.WindowManager
import android.widget.ImageView
import java.lang.ref.WeakReference
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
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
    companion object {
        private var activity = WeakReference<Activity>(null)
        private val guard = Any()
        private val codes = AtomicInteger(24000)
        private class Capture(val code: Int) {
            val done = CountDownLatch(1)
            var bytes: ByteArray? = null
        }
        private var pending: Capture? = null
        private var viewer: AlertDialog? = null
        fun dismissPreview() { synchronized(guard) { viewer }?.dismiss() }
        fun attach(a: Activity) { synchronized(guard) { activity = WeakReference(a) } }
        fun capturePending(code: Int): Boolean = synchronized(guard) { pending?.code == code }
        fun completeCapture(code: Int, bytes: ByteArray?) {
            synchronized(guard) {
                val p = pending
                if (p?.code == code && p.done.count == 1L) { p.bytes = bytes; p.done.countDown() }
                else bytes?.fill(0)
            }
        }

    }
    // In-app CameraX capture; JPEG bytes stay in native process memory.
    fun capturePreview(): ByteArray? {
        if (Looper.myLooper() == Looper.getMainLooper()) return null
        val a = synchronized(guard) { activity.get() } ?: return null
        val p = synchronized(guard) {
            if (pending != null) return null
            val code = codes.getAndIncrement()
            if (code > 64000) return null
            Capture(code).also { pending = it }
        }
        a.runOnUiThread {
            try {
                if (a.isFinishing || a.isDestroyed) p.done.countDown()
                else a.startActivity(Intent(a, CameraCaptureActivity::class.java).putExtra("request_id", p.code))
            } catch (_: Exception) { p.done.countDown() }
        }
        val ok = try { p.done.await(90, TimeUnit.SECONDS) } catch (_: InterruptedException) { false }
        return synchronized(guard) {
            pending = null
            if (!ok) { p.bytes?.fill(0); null } else p.bytes
        }
    }
    // Native-only decoded preview, bounded dimensions, no plaintext file or URI.
    fun showPreview(raw: ByteArray): Boolean {
        if (Looper.myLooper() == Looper.getMainLooper() || raw.size > 16 * 1024 * 1024) { raw.fill(0); return false }
        val a = synchronized(guard) { activity.get() } ?: run { raw.fill(0); return false }
        val bitmap = try {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(raw, 0, raw.size, bounds)
            if (bounds.outWidth !in 1..8192 || bounds.outHeight !in 1..8192 ||
                bounds.outWidth.toLong()*bounds.outHeight > 32_000_000L) return false
            var sample = 1
            while (bounds.outWidth/sample > 2048 || bounds.outHeight/sample > 2048) sample *= 2
            BitmapFactory.decodeByteArray(raw, 0, raw.size, BitmapFactory.Options().apply { inSampleSize = sample })
        } catch (_: Exception) { null } finally { raw.fill(0) } ?: return false
        val done = CountDownLatch(1)
        var shown = false
        val closed = java.util.concurrent.atomic.AtomicBoolean(false)
        a.runOnUiThread {
            if (closed.get() || a.isFinishing || a.isDestroyed || synchronized(guard) { viewer != null }) { bitmap.recycle(); done.countDown(); return@runOnUiThread }
            val view = ImageView(a).apply { setImageBitmap(bitmap); adjustViewBounds = true; contentDescription = "Encrypted photo preview" }
            val dialog = AlertDialog.Builder(a).setTitle("Media preview").setView(view).setPositiveButton("Close", null).create()
            dialog.setOnDismissListener { synchronized(guard) { if (viewer === dialog) viewer = null }; view.setImageDrawable(null); bitmap.recycle(); done.countDown() }
            dialog.window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
            try { synchronized(guard) { viewer = dialog }; dialog.show(); dialog.window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE); shown = true }
            catch (_: Exception) { synchronized(guard) { if (viewer === dialog) viewer = null }; view.setImageDrawable(null); bitmap.recycle(); done.countDown() }
        }
        // A displayed dialog owns its bitmap until dismissal, not until a
        // timeout that would leave a dangling native image reference.
        val completed = try { done.await(120, TimeUnit.SECONDS) } catch (_: InterruptedException) { false }
        if (!completed) { closed.set(true); a.runOnUiThread { dismissPreview() }; return false }
        return shown
    }
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
    // Import only into an absent scope; never overwrite an existing key.
    fun putScopedKey(scope: String, raw: ByteArray): Boolean = synchronized(lock) {
        try {
            val u = UUID.fromString(scope)
            if (u.toString() != scope || u == UUID(0L, 0L) || raw.size != 32 || raw.all { it == 0.toByte() }) return@synchronized false
            if (!root.exists() && !root.mkdirs()) return@synchronized false
            val path = File(root, "$scope.wrap")
            if (path.exists() || File(path.path + ".bak").exists()) {
                val existing = getScopedKey(scope, false) ?: return@synchronized false
                try { return@synchronized java.security.MessageDigest.isEqual(existing, raw) }
                finally { existing.fill(0) }
            }
            val key = master(true) ?: return@synchronized false
            val c = Cipher.getInstance("AES/GCM/NoPadding")
            c.init(Cipher.ENCRYPT_MODE, key)
            c.updateAAD("OVGKEY1:$scope".toByteArray(Charsets.UTF_8))
            val encrypted = c.doFinal(raw)
            if (c.iv.size != 12 || encrypted.size != 48) return@synchronized false
            val atomic = AtomicFile(path)
            val out = atomic.startWrite()
            try { out.write(c.iv); out.write(encrypted); atomic.finishWrite(out) }
            catch (e: Exception) { atomic.failWrite(out); throw e }
            true
        } catch (_: Exception) { false } finally { raw.fill(0) }
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
