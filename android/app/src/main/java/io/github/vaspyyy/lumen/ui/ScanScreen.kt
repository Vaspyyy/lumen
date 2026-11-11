package io.github.vaspyyy.lumen.ui

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import android.util.Size
import android.view.HapticFeedbackConstants
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size as GSize
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import io.github.vaspyyy.lumen.L
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.model.AppModel
import io.github.vaspyyy.lumen.model.Offer
import io.github.vaspyyy.lumen.ui.theme.Lumen
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.concurrent.Executors
import kotlin.math.min

@Composable
fun ScanScreen(model: AppModel, onBack: () -> Unit, onType: () -> Unit) {
    val context = LocalContext.current
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var asked by remember { mutableStateOf(false) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        granted = it
        asked = true
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (granted) {
            Scanner(model)
        } else {
            PermissionAsk(
                denied = asked,
                onAllow = { launcher.launch(Manifest.permission.CAMERA) },
                onSettings = {
                    context.startActivity(
                        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null))
                            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                    )
                },
            )
        }
        // the top bar and the way out, over everything
        Column(Modifier.fillMaxSize().systemBarsPadding()) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RoundIconButton(onBack, background = Color(0x99171522)) {
                    Icon(Icons.back, stringResource(R.string.back), tint = Lumen.fg, modifier = Modifier.size(22.dp))
                }
                Spacer(Modifier.weight(1f))
                Text(stringResource(R.string.scan_title), style = MaterialTheme.typography.titleMedium, color = Lumen.fg)
                Spacer(Modifier.weight(1f))
                if (granted) TorchButton() else Spacer(Modifier.size(44.dp))
            }
            Spacer(Modifier.weight(1f))
            TextButtonLumen(stringResource(R.string.type_code_instead), onType, Modifier.fillMaxWidth().padding(bottom = 12.dp))
        }
    }
}

/** Shared between the scanner and the torch button. */
private val cameraHolder = mutableStateOf<Camera?>(null)

@Composable
private fun TorchButton() {
    var on by remember { mutableStateOf(false) }
    val cam = cameraHolder.value
    val hasFlash = cam?.cameraInfo?.hasFlashUnit() == true
    if (!hasFlash) {
        Spacer(Modifier.size(44.dp))
        return
    }
    RoundIconButton(
        onClick = {
            on = !on
            cam.cameraControl.enableTorch(on)
        },
        background = if (on) Lumen.accent else Color(0x99171522),
    ) {
        Icon(Icons.torch, stringResource(if (on) R.string.torch_off else R.string.torch_on), tint = if (on) Lumen.ink else Lumen.fg, modifier = Modifier.size(22.dp))
    }
}

@Composable
private fun Scanner(model: AppModel) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val view = LocalView.current
    val scope = rememberCoroutineScope()
    var found by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var last by remember { mutableStateOf("") }

    val executor = remember { Executors.newSingleThreadExecutor() }
    val preview = remember {
        PreviewView(context).apply {
            implementationMode = PreviewView.ImplementationMode.COMPATIBLE
            scaleType = PreviewView.ScaleType.FILL_CENTER
        }
    }

    fun onCode(text: String) {
        if (found || text == last) return
        last = text
        scope.launch {
            model.offer(text).onSuccess { offer: Offer ->
                if (found) return@onSuccess
                L.i("scan: found a lumen code")
                found = true
                error = null
                view.performHapticFeedback(HapticFeedbackConstants.CONFIRM)
                delay(520)
                model.startPairing(text.trim(), offer)
            }.onFailure {
                error = it.message
                view.performHapticFeedback(HapticFeedbackConstants.REJECT)
            }
        }
    }

    DisposableEffect(owner) {
        val future = ProcessCameraProvider.getInstance(context)
        var provider: ProcessCameraProvider? = null
        future.addListener({
            val p = runCatching { future.get() }.getOrNull() ?: return@addListener
            provider = p
            val pre = Preview.Builder().build().also { it.surfaceProvider = preview.surfaceProvider }
            val analysis = ImageAnalysis.Builder()
                .setResolutionSelector(
                    ResolutionSelector.Builder()
                        .setResolutionStrategy(ResolutionStrategy(Size(1280, 720), ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER))
                        .build(),
                )
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
            analysis.setAnalyzer(executor, QrAnalyzer { text -> preview.post { onCode(text) } })
            runCatching {
                p.unbindAll()
                cameraHolder.value = p.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, pre, analysis)
            }.onFailure { L.w("scan: could not open the camera", it) }
        }, ContextCompat.getMainExecutor(context))
        onDispose {
            provider?.unbindAll()
            cameraHolder.value = null
            executor.shutdown()
        }
    }

    // forget the last bad code after a while, so it can be tried again
    LaunchedEffect(error) {
        if (error != null) {
            delay(4000)
            last = ""
        }
    }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        AndroidView({ preview }, Modifier.fillMaxSize())
        val side = min(maxWidth.value, maxHeight.value) * 0.72f
        val scale by animateFloatAsState(if (found) 0.92f else 1f, spring(0.55f, Spring.StiffnessMediumLow), label = "vf")
        val corner by animateColorAsState(if (found) Lumen.accent else Color.White.copy(alpha = 0.92f), spring(stiffness = Spring.StiffnessMediumLow), label = "c")
        val closed by animateFloatAsState(if (found) 1f else 0f, spring(0.7f, Spring.StiffnessLow), label = "closed")
        Canvas(Modifier.fillMaxSize()) {
            val s = side.dp.toPx() * scale
            val left = (size.width - s) / 2f
            val top = (size.height - s) / 2f - 24.dp.toPx()
            val r = 28.dp.toPx()
            val hole = Path().apply {
                fillType = PathFillType.EvenOdd
                addRect(androidx.compose.ui.geometry.Rect(0f, 0f, size.width, size.height))
                addRoundRect(RoundRect(left, top, left + s, top + s, CornerRadius(r)))
            }
            drawPath(hole, Color(0xB30B0A11))
            if (closed > 0f) {
                drawRoundRect(
                    Lumen.accent.copy(alpha = 0.16f * closed),
                    Offset(left, top), GSize(s, s), CornerRadius(r),
                )
            }
            // corner brackets that close into a full frame when a code is found
            val len = s * (0.16f + 0.34f * closed)
            val w = 4.dp.toPx()
            val stroke = Stroke(w, cap = StrokeCap.Round)
            fun bracket(x: Float, y: Float, dx: Float, dy: Float) {
                val p = Path().apply {
                    moveTo(x, y + dy * len)
                    lineTo(x, y + dy * r)
                    quadraticTo(x, y, x + dx * r, y)
                    lineTo(x + dx * len, y)
                }
                drawPath(p, corner, style = stroke)
            }
            bracket(left, top, 1f, 1f)
            bracket(left + s, top, -1f, 1f)
            bracket(left, top + s, 1f, -1f)
            bracket(left + s, top + s, -1f, -1f)
        }
        Column(
            Modifier
                .align(Alignment.Center)
                .offset(y = (side / 2f + 28f).dp)
                .padding(horizontal = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                stringResource(if (found) R.string.scan_found else R.string.scan_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = if (found) Lumen.accent else Lumen.fg2,
                textAlign = TextAlign.Center,
            )
            AnimatedVisibility(error != null && !found, enter = fadeIn() + slideInVertically { it / 2 }, exit = fadeOut() + slideOutVertically { it / 2 }) {
                Text(
                    error.orEmpty(),
                    style = MaterialTheme.typography.bodyMedium,
                    color = Lumen.pink,
                    textAlign = TextAlign.Center,
                    modifier = Modifier
                        .padding(top = 14.dp)
                        .clip(RoundedCornerShape(14.dp))
                        .background(Color(0xE6171522))
                        .padding(horizontal = 14.dp, vertical = 10.dp),
                )
            }
        }
    }
}

@Composable
private fun PermissionAsk(denied: Boolean, onAllow: () -> Unit, onSettings: () -> Unit) {
    Column(
        Modifier.fillMaxSize().background(Lumen.bg).systemBarsPadding().padding(horizontal = 28.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text(stringResource(R.string.camera_title), style = MaterialTheme.typography.displaySmall, color = Lumen.fg)
        Spacer(Modifier.height(14.dp))
        Text(stringResource(R.string.camera_body), style = MaterialTheme.typography.bodyLarge, color = Lumen.fg3)
        Spacer(Modifier.height(28.dp))
        if (denied) {
            Text(stringResource(R.string.camera_denied), style = MaterialTheme.typography.bodyMedium, color = Lumen.fg3)
            Spacer(Modifier.height(16.dp))
            PrimaryButton(stringResource(R.string.open_settings), onSettings, Modifier.fillMaxWidth())
            Spacer(Modifier.height(8.dp))
            QuietButton(stringResource(R.string.camera_allow), onAllow, Modifier.fillMaxWidth())
        } else {
            PrimaryButton(stringResource(R.string.camera_allow), onAllow, Modifier.fillMaxWidth())
        }
    }
}

/** Reads QR codes from camera frames, off the main thread. Tries the inverted image too: terminals often draw codes light on dark. */
private class QrAnalyzer(private val onText: (String) -> Unit) : ImageAnalysis.Analyzer {
    private val reader = QRCodeReader()
    private val hints = mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE))
    private var data = ByteArray(0)

    override fun analyze(image: ImageProxy) {
        try {
            val plane = image.planes[0]
            val buf = plane.buffer
            val stride = plane.rowStride
            val w = image.width
            val h = image.height
            val need = stride * (h - 1) + w
            if (data.size < need) data = ByteArray(need)
            buf.rewind()
            buf.get(data, 0, min(need, buf.remaining()))
            val side = min(w, h)
            val src = PlanarYUVLuminanceSource(data, stride, h, (w - side) / 2, (h - side) / 2, side, side, false)
            val text = decode(src) ?: decode(src.invert())
            if (text != null) onText(text)
        } catch (_: Exception) {
        } finally {
            image.close()
        }
    }

    private fun decode(src: com.google.zxing.LuminanceSource): String? = try {
        reader.decode(BinaryBitmap(HybridBinarizer(src)), hints).text
    } catch (_: ReaderException) {
        null
    } finally {
        reader.reset()
    }
}
