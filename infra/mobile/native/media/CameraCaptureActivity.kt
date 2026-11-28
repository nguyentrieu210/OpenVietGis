package org.openvietgis.secure

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.ImageFormat
import android.os.Bundle
import android.view.Gravity
import android.view.WindowManager
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageCapture
import androidx.camera.core.ImageCaptureException
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import java.util.concurrent.atomic.AtomicBoolean

/** One still image directly from CameraX, no gallery or plaintext file. */
class CameraCaptureActivity : ComponentActivity() {
    private val finished = AtomicBoolean(false)
    private var provider: ProcessCameraProvider? = null
    private var capture: ImageCapture? = null
    private lateinit var preview: PreviewView
    private lateinit var button: Button
    private lateinit var message: TextView
    private val requestCodeId by lazy { intent.getIntExtra("request_id", -1) }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        if (requestCodeId < 0 || !MediaKeys.capturePending(requestCodeId)) {
            finish()
            return
        }
        preview = PreviewView(this)
        val frame = FrameLayout(this)
        frame.addView(preview, FrameLayout.LayoutParams(-1, -1))
        val controls = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(0xCCFFFFFF.toInt())
        }
        message = TextView(this).apply { text = "Camera is starting…" }
        button = Button(this).apply { text = "Capture JPEG"; isEnabled = false }
        val cancel = Button(this).apply { text = "Cancel"; setOnClickListener { complete(null) } }
        controls.addView(message); controls.addView(button); controls.addView(cancel)
        frame.addView(controls, FrameLayout.LayoutParams(-1, -2, Gravity.BOTTOM))
        setContentView(frame)
        button.setOnClickListener { takePhoto() }
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) startCamera()
        else requestPermissions(arrayOf(Manifest.permission.CAMERA), 100)
    }
    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, results: IntArray) {
        super.onRequestPermissionsResult(code, permissions, results)
        if (code == 100) {
            if (results.firstOrNull() == PackageManager.PERMISSION_GRANTED) startCamera()
            else complete(null)
        }
    }
    private fun startCamera() {
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            if (finished.get() || isDestroyed || !MediaKeys.capturePending(requestCodeId)) {
                complete(null)
                return@addListener
            }
            try {
                val p = future.get()
                provider = p
                val view = Preview.Builder().build().also { it.setSurfaceProvider(preview.surfaceProvider) }
                val still = ImageCapture.Builder()
                    .setCaptureMode(ImageCapture.CAPTURE_MODE_MAXIMIZE_QUALITY)
                    .setJpegQuality(92)
                    .build()
                p.bindToLifecycle(this, CameraSelector.DEFAULT_BACK_CAMERA, view, still)
                capture = still
                message.text = "JPEG in memory · no photo file"
                button.isEnabled = true
            } catch (_: Exception) { complete(null) }
        }, ContextCompat.getMainExecutor(this))
    }
    private fun takePhoto() {
        val still = capture ?: return
        if (!MediaKeys.capturePending(requestCodeId)) { complete(null); return }
        button.isEnabled = false
        preview.display?.let { still.targetRotation = it.rotation }
        still.takePicture(ContextCompat.getMainExecutor(this), object : ImageCapture.OnImageCapturedCallback() {
            override fun onCaptureSuccess(image: ImageProxy) {
                var data: ByteArray? = null
                try {
                    if (image.format == ImageFormat.JPEG && image.width in 1..8192 && image.height in 1..8192 &&
                        image.width.toLong() * image.height <= 32_000_000L && image.planes.size == 1) {
                        val buffer = image.planes[0].buffer
                        if (buffer.remaining() in 4..(16 * 1024 * 1024)) {
                            data = ByteArray(buffer.remaining()).also { buffer.get(it) }
                        }
                    }
                } catch (_: Exception) { data?.fill(0); data = null }
                finally { image.close() }
                complete(data)
            }
            override fun onError(exception: ImageCaptureException) { complete(null) }
        })
    }
    private fun complete(bytes: ByteArray?) {
        if (!finished.compareAndSet(false, true)) { bytes?.fill(0); return }
        provider?.unbindAll()
        MediaKeys.completeCapture(requestCodeId, bytes)
        finish()
    }
    override fun onDestroy() {
        if (finished.compareAndSet(false, true)) MediaKeys.completeCapture(requestCodeId, null)
        super.onDestroy()
    }
}
