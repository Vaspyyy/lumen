package io.github.vaspyyy.lumen.data

import android.content.Context
import android.util.AtomicFile
import androidx.core.content.edit
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** A PC this phone is paired with. */
data class Pc(
    val id: String,
    val label: String,
    val user: String,
    /** The host object `Core.enroll` returned, verbatim, as a JSON string: what `Core.connect` takes. */
    val host: String,
    /** The Android Keystore alias of this PC's key. */
    val keyAlias: String,
    val addresses: List<String>,
    /** The kept terminals this phone has open on the PC, to reattach when it connects again. */
    val tabs: List<String>,
) {
    fun toJson(): JSONObject = JSONObject()
        .put("id", id)
        .put("label", label)
        .put("user", user)
        .put("host", JSONObject(host))
        .put("key", keyAlias)
        .put("addresses", JSONArray(addresses))
        .put("tabs", JSONArray(tabs))

    companion object {
        fun fromJson(o: JSONObject) = Pc(
            id = o.getString("id"),
            label = o.optString("label"),
            user = o.optString("user"),
            host = o.getJSONObject("host").toString(),
            keyAlias = o.getString("key"),
            addresses = o.optJSONArray("addresses").strings(),
            tabs = o.optJSONArray("tabs").strings(),
        )
    }
}

fun JSONArray?.strings(): List<String> =
    if (this == null) emptyList() else List(length()) { optString(it) }.filter { it.isNotEmpty() }

/**
 * The paired PCs and the app's few settings, in the app's private storage.
 * Excluded from every backup (see the manifest): nothing here is useful
 * without the keys, which never leave the phone.
 */
class Store(context: Context) {
    private val file = AtomicFile(File(context.filesDir, "pcs.json"))
    private val prefs = context.getSharedPreferences("lumen", Context.MODE_PRIVATE)

    @Synchronized
    fun load(): List<Pc> = runCatching {
        if (!file.baseFile.exists()) return emptyList()
        val root = JSONObject(String(file.readFully(), Charsets.UTF_8))
        val arr = root.optJSONArray("pcs") ?: return emptyList()
        List(arr.length()) { Pc.fromJson(arr.getJSONObject(it)) }
    }.getOrElse { emptyList() }

    @Synchronized
    fun save(pcs: List<Pc>) {
        val root = JSONObject().put("version", 1).put("pcs", JSONArray(pcs.map { it.toJson() }))
        val out = file.startWrite()
        try {
            out.write(root.toString().toByteArray(Charsets.UTF_8))
            file.finishWrite(out)
        } catch (e: Exception) {
            file.failWrite(out)
            throw e
        }
    }

    var currentPc: String?
        get() = prefs.getString("current", null)
        set(v) = prefs.edit { putString("current", v) }

    var fontSize: Float
        get() = prefs.getFloat("font_size", 13f)
        set(v) = prefs.edit { putFloat("font_size", v) }
}
