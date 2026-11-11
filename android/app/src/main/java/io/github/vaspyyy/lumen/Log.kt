package io.github.vaspyyy.lumen

import android.util.Log

/**
 * Logs under the tag `lumen`, for following pairing and connecting with
 * `adb logcat -s lumen`. Never pass a pairing code or key material here.
 */
object L {
    private const val TAG = "lumen"
    fun i(msg: String) { Log.i(TAG, msg) }
    fun w(msg: String, t: Throwable? = null) { Log.w(TAG, msg, t) }
    fun e(msg: String, t: Throwable? = null) { Log.e(TAG, msg, t) }
}
