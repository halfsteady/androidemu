package dev.androidemu

import android.content.Context
import android.util.Base64
import androidx.compose.runtime.mutableStateOf
import androidx.core.content.edit

/**
 * The handful of choices that outlive a session. Each one is Compose state so the
 * UI follows it, and each write goes straight to disk so a crash never loses a
 * preference the player just set.
 *
 * Control size is not here. It follows the screen instead of asking, and
 * fast-forward speed is not here either — it comes from how far the time control
 * is pulled.
 */
class Settings(context: Context) {
    private val preferences = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    private val aspectState = mutableStateOf(
        Aspect.entries.getOrElse(preferences.getInt(ASPECT, 0)) { Aspect.Television }
    )
    var aspect: Aspect
        get() = aspectState.value
        set(value) { aspectState.value = value; preferences.edit { putInt(ASPECT, value.ordinal) } }

    private val trimEdgesState = mutableStateOf(preferences.getBoolean(TRIM_EDGES, false))
    var trimEdges: Boolean
        get() = trimEdgesState.value
        set(value) { trimEdgesState.value = value; preferences.edit { putBoolean(TRIM_EDGES, value) } }

    // The first release had a scanlines on/off switch. Anyone who had it on keeps
    // the look they chose, now as one filter among several.
    private val filterState = mutableStateOf(
        if (preferences.contains(FILTER)) Filter.of(preferences.getInt(FILTER, 0))
        else if (preferences.getBoolean(SCANLINES, false)) Filter.Scanlines
        else Filter.None
    )
    var filter: Filter
        get() = filterState.value
        set(value) { filterState.value = value; preferences.edit { putInt(FILTER, value.id) } }

    private val paletteState = mutableStateOf(Palette.of(preferences.getInt(PALETTE, 0)))
    var palette: Palette
        get() = paletteState.value
        set(value) { paletteState.value = value; preferences.edit { putInt(PALETTE, value.id) } }

    /**
     * The imported `.pal` table, 192 bytes, or null if none was ever imported.
     * Kept here rather than as a file because it is smaller than the path would
     * be, and because losing it should cost one re-import and nothing else.
     */
    private val importedState = mutableStateOf(
        preferences.getString(IMPORTED_PALETTE, null)?.let {
            runCatching { Base64.decode(it, Base64.DEFAULT) }.getOrNull()
        }
    )
    var importedPalette: ByteArray?
        get() = importedState.value
        set(value) {
            importedState.value = value
            preferences.edit {
                if (value == null) remove(IMPORTED_PALETTE)
                else putString(IMPORTED_PALETTE, Base64.encodeToString(value, Base64.NO_WRAP))
            }
        }

    /**
     * The 192 bytes to hand the core for the current choice, or an empty array
     * for the core's own table. An imported palette that has gone missing falls
     * back rather than painting nothing.
     */
    fun paletteBytes(): ByteArray {
        if (palette == Palette.File) {
            val imported = importedPalette ?: return ByteArray(0)
            return PaletteModel.bytes(PaletteModel.parse(imported) ?: return ByteArray(0))
        }
        return palette.colours()?.let { PaletteModel.bytes(it) } ?: ByteArray(0)
    }

    /**
     * The colours the picture is being painted with, resolved. [Palette.Standard]
     * has no table of its own — it is the core's — so the model's closest match
     * stands in wherever a table is needed on this side, such as the built
     * preview pattern.
     */
    fun paletteColours(): IntArray {
        if (palette == Palette.File) {
            importedPalette?.let { file -> PaletteModel.parse(file)?.let { return it } }
            return PaletteModel.build()
        }
        return palette.colours() ?: PaletteModel.standard()
    }

    private companion object {
        const val ASPECT = "aspect"
        const val PALETTE = "palette"
        const val IMPORTED_PALETTE = "importedPalette"
        const val TRIM_EDGES = "trimEdges"
        const val FILTER = "filter"
        const val SCANLINES = "scanlines"
    }
}
