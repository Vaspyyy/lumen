package io.github.vaspyyy.lumen

import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.fragment.app.FragmentActivity
import io.github.vaspyyy.lumen.keys.Authenticator
import io.github.vaspyyy.lumen.model.AppModel
import io.github.vaspyyy.lumen.ui.LumenApp
import io.github.vaspyyy.lumen.ui.theme.LumenTheme

/** The one activity. A FragmentActivity because the system's fingerprint prompt needs one. */
class MainActivity : FragmentActivity() {
    private val model: AppModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        super.onCreate(savedInstanceState)
        setContent { LumenTheme { LumenApp(model) } }
        model.start(if (savedInstanceState == null) pairingLink(intent) else null)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        // Never used silently: the model asks the user first.
        pairingLink(intent)?.let(model::receiveLink)
    }

    // The system's prompt can be shown only while the activity is started.
    override fun onStart() {
        super.onStart()
        Authenticator.attach(this)
        model.onFront()
    }

    override fun onStop() {
        Authenticator.detach(this)
        super.onStop()
    }

    /** The pairing link an intent carries, if it carries one that was not used before. */
    private fun pairingLink(intent: Intent?): String? {
        if (intent?.action != Intent.ACTION_VIEW) return null
        if (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return null
        return intent.dataString?.takeIf { it.startsWith("lumen://pair", ignoreCase = true) }
    }
}
