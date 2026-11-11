package io.github.vaspyyy.lumen.keys

import android.os.Handler
import android.os.Looper
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.Lifecycle
import io.github.vaspyyy.lumen.L
import io.github.vaspyyy.lumen.core.Signer
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import java.lang.ref.WeakReference
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume

/**
 * Shows the system prompt (fingerprint, face, or the screen lock) that lets
 * a key sign for the next 20 seconds. The key is time-bound, so the prompt
 * needs no CryptoObject: any strong proof of the owner unlocks it.
 */
object Authenticator {
    @Volatile
    private var activity: WeakReference<FragmentActivity>? = null
    private val main = Handler(Looper.getMainLooper())
    /** How long a prompt may stay unanswered. */
    private const val PATIENCE_MS = 120_000L

    /** Somebody wanted the owner while the app was not in front, where no prompt can be shown. */
    @Volatile
    private var missed = false

    /** Set by the activity while it is in front (started), where a prompt can be shown. */
    fun attach(a: FragmentActivity) {
        activity = WeakReference(a)
    }

    fun detach(a: FragmentActivity) {
        if (activity?.get() === a) activity = null
    }

    /** Whether the owner was wanted while the app was away, since this was last asked. */
    fun takeMissed(): Boolean {
        val was = missed
        missed = false
        return was
    }

    /**
     * Must be called on the main thread. True when the owner proved who they
     * are. False at once when the app is not in front, and after two minutes
     * without an answer.
     */
    suspend fun authenticate(title: String, subtitle: String): Boolean = withTimeoutOrNull(PATIENCE_MS) {
        suspendCancellableCoroutine { cont ->
            val prompt = show(title, subtitle) { ok -> if (cont.isActive) cont.resume(ok) }
            // given up on (the connection was dropped, time ran out): the prompt goes too
            cont.invokeOnCancellation { main.post { runCatching { prompt?.cancelAuthentication() } } }
        }
    } ?: false

    /** For background threads: blocks until the prompt is answered (or 2 minutes pass). */
    fun authenticateBlocking(title: String, subtitle: String): Boolean {
        check(Looper.myLooper() != Looper.getMainLooper()) { "authenticateBlocking on the main thread" }
        val latch = CountDownLatch(1)
        var result = false
        var prompt: BiometricPrompt? = null
        main.post {
            prompt = show(title, subtitle) { ok ->
                result = ok
                latch.countDown()
            }
        }
        if (!latch.await(PATIENCE_MS, TimeUnit.MILLISECONDS)) {
            main.post { runCatching { prompt?.cancelAuthentication() } }
            return false
        }
        return result
    }

    /** Shows the prompt, or says no at once where none can be shown. Main thread only. */
    private fun show(title: String, subtitle: String, done: (Boolean) -> Unit): BiometricPrompt? {
        val a = activity?.get()
        // A prompt asked for while the activity is stopped is dropped without a
        // word: nobody would ever answer. The app asks again when it is back.
        if (a == null || a.isFinishing || a.isDestroyed ||
            !a.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED) || a.supportFragmentManager.isStateSaved
        ) {
            L.w("auth: the app is not in front, nobody to ask")
            missed = true
            done(false)
            return null
        }
        L.i("auth: asking the owner")
        val prompt = BiometricPrompt(a, ContextCompat.getMainExecutor(a), object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                L.i("auth: succeeded")
                done(true)
            }

            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                L.i("auth: ended without success ($errorCode)")
                // taken away by the system because the app left the front: not the owner's no
                if (errorCode == BiometricPrompt.ERROR_CANCELED) missed = true
                done(false)
            }
            // onAuthenticationFailed: a finger that did not match; the prompt stays and lets them try again.
        })
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(title)
            .setSubtitle(subtitle)
            .setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
            .setConfirmationRequired(false)
            .build()
        prompt.authenticate(info)
        return prompt
    }
}

/** The Signer the core calls while it logs in: signs with the PC's key, asking the owner first if the key needs it. */
class KeySigner(private val alias: String, private val title: String, private val subtitle: String) : Signer {
    override fun sign(data: ByteArray): ByteArray? {
        when (val first = KeyVault.sign(alias, data)) {
            is KeyVault.Signed.Ok -> return first.signature
            KeyVault.Signed.NeedsAuth -> Unit
            else -> return null
        }
        L.i("sign: the key needs the owner again")
        if (!Authenticator.authenticateBlocking(title, subtitle)) return null
        return (KeyVault.sign(alias, data) as? KeyVault.Signed.Ok)?.signature
    }
}
