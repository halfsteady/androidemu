package dev.androidemu

import org.junit.Assert.*
import org.junit.Test

/**
 * The filter list and the pattern the settings preview falls back on. The shader
 * itself needs a GPU, but its inputs do not: an ordinal that shifts silently
 * changes everybody's saved look, and a flat sample would preview nothing.
 */
class LookTest {
    @Test fun theOrdinalsAreTheSavedValuesSoTheyMustNotMove() {
        // Persisted in preferences. Appending is fine; reordering is not.
        assertEquals(0, Filter.None.ordinal)
        assertEquals(1, Filter.Scanlines.ordinal)
        assertEquals(2, Filter.Crt.ordinal)
        assertEquals(3, Filter.DotMatrix.ordinal)
        assertEquals(4, Filter.GameBoy.ordinal)
        assertEquals(5, Filter.Grey.ordinal)
        assertEquals(6, Filter.Sepia.ordinal)
        assertEquals(7, Filter.Neon.ordinal)
    }

    @Test fun anUnknownOrdinalFallsBackRatherThanCrashing() {
        assertEquals(Filter.Crt, Filter.of(2))
        assertEquals(Filter.None, Filter.of(99))
        assertEquals(Filter.None, Filter.of(-1))
    }

    @Test fun everyLookIsNamedAndExplained() {
        for (filter in Filter.entries) {
            assertTrue(filter.name, filter.label.isNotBlank())
            assertTrue(filter.name, filter.note.isNotBlank())
        }
        assertEquals("no two looks share a name", Filter.entries.size, Filter.entries.map { it.label }.distinct().size)
    }

    @Test fun theSampleFrameIsAWholeOpaqueFramebuffer() {
        val pixels = SampleFrame.pixels()
        assertEquals(Picture.WIDTH * Picture.HEIGHT * 4, pixels.size)
        for (at in 3 until pixels.size step 4) {
            assertEquals("alpha at $at", 255, pixels[at].toInt() and 255)
        }
    }

    @Test fun theSampleFrameHasSomethingForEveryFilterToChew() {
        val pixels = SampleFrame.pixels()
        fun luma(x: Int, y: Int): Int {
            val at = (y * Picture.WIDTH + x) * 4
            return (pixels[at].toInt() and 255) + (pixels[at + 1].toInt() and 255) + (pixels[at + 2].toInt() and 255)
        }
        // Colour, so palette and saturation filters have something to move.
        val bars = (0 until Picture.WIDTH step 8).map { luma(it, 20) }.distinct()
        assertTrue("colour bars are flat", bars.size > 4)
        // A ramp, for the contrast filters.
        assertTrue("no ramp", luma(240, 100) > luma(8, 100))
        // Alternating rows, which is what scanlines and the LCD grid sit on.
        assertNotEquals(luma(10, 140), luma(11, 140))
        // Bright blocks on a dark field, for the glow. Scanned rather than
        // sampled at a guessed coordinate, since that is the actual property:
        // somewhere in the bottom band is bright and somewhere is dark.
        val band = (168 until Picture.HEIGHT step 4).flatMap { y -> (0 until Picture.WIDTH step 4).map { luma(it, y) } }
        assertTrue("nothing bright to bloom", band.max() > band.min() + 300)
    }
}
