package io.github.vaspyyy.lumen.data

import android.content.Context
import android.os.Build
import android.provider.Settings

object Device {
    /** The phone's name as its owner sees it ("Galaxy S24 Ultra"), for the PC to show when it asks. */
    fun name(context: Context): String {
        val set = runCatching { Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME) }.getOrNull()
        if (!set.isNullOrBlank()) return set.trim()
        val maker = Build.MANUFACTURER.orEmpty().replaceFirstChar { it.uppercase() }
        val model = Build.MODEL.orEmpty()
        return if (model.startsWith(maker, ignoreCase = true)) model else "$maker $model".trim()
    }

    /** A short lowercase form of the model for terminal names: "Galaxy S24 Ultra" becomes "s24-ultra". */
    fun slug(context: Context): String {
        var s = name(context).lowercase()
        for (brand in listOf("samsung ", "galaxy ", "google ")) s = s.removePrefix(brand)
        s = s.replace(Regex("[^a-z0-9]+"), "-").trim('-')
        if (s.length > 20) s = s.substring(0, 20).trimEnd('-')
        return s.ifEmpty { "phone" }
    }
}
