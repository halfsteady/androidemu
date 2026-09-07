package dev.androidemu

import kotlin.math.abs
import kotlin.math.ceil

/**
 * One control runs time in both directions: drag left of centre to go back, right
 * to go forward, and the further from centre the faster it runs. This is the
 * mapping from where the finger is to how many emulated frames a displayed frame
 * is worth, kept as a pure function because it is the part that has to feel right.
 */
object Scrub {
    /** Frames per displayed frame at the far end of the track. */
    const val MAX = 8

    /**
     * A finger resting slightly off centre should not creep the game along, so the
     * middle of the track is dead. It is generous, because the thumb springs back
     * to centre and a player expects "roughly the middle" to mean stopped.
     */
    const val DEAD_ZONE = 0.12f

    /**
     * [fraction] runs -1 (fully left) to 1 (fully right). Negative results are
     * frames to step back per displayed frame, positive are frames to run forward,
     * and zero is ordinary play.
     */
    fun speed(fraction: Float): Int {
        val magnitude = abs(fraction).coerceAtMost(1f)
        if (magnitude <= DEAD_ZONE) return 0
        val past = (magnitude - DEAD_ZONE) / (1f - DEAD_ZONE)
        val steps = ceil(past * MAX).toInt().coerceIn(1, MAX)
        return if (fraction < 0) -steps else steps
    }

    /** What the heads-up display says while the track is held. */
    fun label(speed: Int) = when {
        speed < 0 -> "Rewinding ${-speed}×"
        speed > 0 -> "Fast-forward ${speed}×"
        else -> "Ready"
    }
}
