package dev.androidemu

import org.junit.Assert.*
import org.junit.Test

/**
 * The feel of the one time control. This is the part a player notices, so it is a
 * pure function with tests rather than arithmetic buried in a gesture handler.
 */
class ScrubTest {
    @Test fun theMiddleOfTheTrackIsStopped() {
        assertEquals(0, Scrub.speed(0f))
        assertEquals(0, Scrub.speed(Scrub.DEAD_ZONE))
        assertEquals(0, Scrub.speed(-Scrub.DEAD_ZONE))
        // Just past the dead zone is the slowest step, never a jump.
        assertEquals(1, Scrub.speed(Scrub.DEAD_ZONE + 0.001f))
        assertEquals(-1, Scrub.speed(-Scrub.DEAD_ZONE - 0.001f))
    }

    @Test fun theEndsOfTheTrackAreTheFastest() {
        assertEquals(Scrub.MAX, Scrub.speed(1f))
        assertEquals(-Scrub.MAX, Scrub.speed(-1f))
        // Past the end is clamped, not wrapped: a finger dragged off the control
        // must not come back as a slow crawl.
        assertEquals(Scrub.MAX, Scrub.speed(3f))
        assertEquals(-Scrub.MAX, Scrub.speed(-3f))
    }

    @Test fun speedRisesWithDistanceAndNeverSkipsBackwards() {
        var previous = 0
        var at = 0f
        while (at <= 1f) {
            val speed = Scrub.speed(at)
            assertTrue("$at went from $previous to $speed", speed >= previous)
            assertTrue("$at gave $speed", speed in 0..Scrub.MAX)
            // Left is the mirror of right, all the way along.
            assertEquals(-speed, Scrub.speed(-at))
            previous = speed
            at += 0.01f
        }
        assertEquals(Scrub.MAX, previous)
    }

    @Test fun everyStepBetweenOneAndTheMaximumIsReachable() {
        val reached = (0..100).map { Scrub.speed(it / 100f) }.toSet()
        assertEquals((0..Scrub.MAX).toSet(), reached)
    }

    @Test fun theLabelSaysWhichWayTimeIsGoing() {
        assertEquals("Ready", Scrub.label(0))
        assertEquals("Rewinding 3×", Scrub.label(-3))
        assertEquals("Fast-forward 8×", Scrub.label(8))
    }
}
