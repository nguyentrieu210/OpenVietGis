package org.openvietgis.secure

import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Looper
import android.provider.MediaStore
import android.view.WindowManager
import android.widget.ImageView
import java.io.ByteArrayOutputStream
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
        fun attach(a: Activity) { synchronized(guard) { activity = WeakReference(a) } }
        fun cameraResult(code: Int, result: Int, data: Intent?): Boolean {
            val p = synchronized(guard) { pending?.takeIf { it.code == code } } ?: return false
            var bytes: ByteArray? = null
            try {
                if (result == Activity.RESULT_OK) {
                    @Suppress("DEPRECATION")
                    val bitmap = data?.extras?.get("data") as? Bitmap
                    if (bitmap != null && bitmap.width in 1..4096 && bitmap.height in 1..4096 &&
                        bitmap.width.toLong() * bitmap.height <= 4_000_000L) {
                        val out = ByteArrayOutputStream()
                        if (bitmap.compress(Bitmap.CompressFormat.JPEG, 85, out)) {
                            val raw = out.toByteArray()
                            if (raw.size <= 16 * 1024 * 1024) bytes = raw else raw.fill(0)
                        }
                    }
                }
            } catch (_: Exception) { bytes?.fill(0); bytes = null }
            synchronized(guard) {
                if (pending === p) { p.bytes = bytes; p.done.countDown() }
                else bytes?.fill(0)
            }
            return true
        }
    }
    // ACTION_IMAGE_CAPTURE preview only; no output URI or gallery write requested.
    // Native camera app itself is outside our process and trust boundary.
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
                else a.startActivityForResult(Intent(MediaStore.ACTION_IMAGE_CAPTURE), p.code)
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
            if (closed.get() || a.isFinishing || a.isDestroyed) { bitmap.recycle(); done.countDown(); return@runOnUiThread }
            val view = ImageView(a).apply { setImageBitmap(bitmap); adjustViewBounds = true; contentDescription = "Encrypted photo preview" }
            val dialog = AlertDialog.Builder(a).setTitle("Media preview").setView(view).setPositiveButton("Close", null).create()
            dialog.setOnDismissListener { view.setImageDrawable(null); bitmap.recycle(); done.countDown() }
            dialog.window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
            try { dialog.show(); dialog.window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE); shown = true }
            catch (_: Exception) { view.setImageDrawable(null); bitmap.recycle(); done.countDown() }
        }
        // A displayed dialog owns its bitmap until dismissal, not until a
        // timeout that would leave a dangling native image reference.
        try { done.await() } catch (_: InterruptedException) { closed.set(true); return false }
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
