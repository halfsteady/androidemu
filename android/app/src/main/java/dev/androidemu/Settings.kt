package dev.androidemu

import android.content.Context
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.core.content.edit

/**
 * The handful of choices that outlive a session. Each one is Compose state so the
 * UI follows it, and each write goes straight to disk so a crash never loses a
 * preference the player just set.
 */
class Settings(context: Context) {
    private val preferences = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    private val bigControlsState = mutableStateOf(preferences.getBoolean(BIG_CONTROLS, false))
    /** Larger touch targets. Worth having on a 13" screen whoever is holding it. */
    var bigControls: Boolean
        get() = bigControlsState.value
        set(value) { bigControlsState.value = value; preferences.edit { putBoolean(BIG_CONTROLS, value) } }

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

    private val scanlinesState = mutableStateOf(preferences.getBoolean(SCANLINES, false))
    var scanlines: Boolean
        get() = scanlinesState.value
        set(value) { scanlinesState.value = value; preferences.edit { putBoolean(SCANLINES, value) } }

    private val fastForwardState = mutableIntStateOf(preferences.getInt(FAST_FORWARD, 4))
    /** How many frames a held fast-forward runs per displayed frame. */
    var fastForward: Int
        get() = fastForwardState.value
        set(value) { fastForwardState.value = value; preferences.edit { putInt(FAST_FORWARD, value) } }

    /** Cycles the speed rather than offering a slider: three choices cover it. */
    fun cycleFastForward() { fastForward = SPEEDS[(SPEEDS.indexOf(fastForward) + 1).mod(SPEEDS.size)] }

    private companion object {
        const val BIG_CONTROLS = "bigControls"
        const val ASPECT = "aspect"
        const val TRIM_EDGES = "trimEdges"
        const val SCANLINES = "scanlines"
        const val FAST_FORWARD = "fastForward"
        val SPEEDS = listOf(2, 4, 8)
    }
}
