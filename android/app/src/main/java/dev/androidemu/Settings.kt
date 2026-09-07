package dev.androidemu

import android.content.Context
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
        set(value) { filterState.value = value; preferences.edit { putInt(FILTER, value.ordinal) } }

    private companion object {
        const val ASPECT = "aspect"
        const val TRIM_EDGES = "trimEdges"
        const val FILTER = "filter"
        const val SCANLINES = "scanlines"
    }
}
