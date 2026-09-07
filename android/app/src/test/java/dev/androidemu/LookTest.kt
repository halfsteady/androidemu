package dev.androidemu

import org.junit.Assert.*
import org.junit.Test

/**
 * The filter list and the pattern the settings preview falls back on. The shader
 * itself needs a GPU, but its inputs do not: an ordinal that shifts silently
 * changes everybody's saved look, and a flat sample would preview nothing.
 */
class LookTest {
    @Test fun theIdsAreTheSavedValuesSoTheyMustNotMove() {
        // Persisted in preferences. These are the numbers on disk, so a new look
        // may take the next free one and a cut look's number is never reissued.
        assertEquals(0, Filter.None.id)
        assertEquals(1, Filter.Scanlines.id)
        assertEquals(2, Filter.Crt.id)
        assertEquals(3, Filter.DotMatrix.id)
        assertEquals(4, Filter.GameBoy.id)
        assertEquals(6, Filter.Sepia.id)
        assertEquals(7, Filter.Neon.id)
        assertEquals(8, Filter.Cartoon.id)
        assertEquals("ids must be unique", Filter.entries.size, Filter.entries.map { it.id }.distinct().size)
    }

    @Test fun theCutBlackAndWhiteLandsOnTheLookThatAbsorbedIt() {
        // 5 was "Black and white", which was Sepia with the tint thrown away.
        // Anyone still saved on it gets Sepia rather than silently losing it,
        // and 5 is never handed to a new look.
        assertEquals(Filter.Sepia, Filter.of(5))
        assertFalse("id 5 is retired", Filter.entries.any { it.id == 5 })
    }

    @Test fun anUnknownIdFallsBackRatherThanCrashing() {
        assertEquals(Filter.Crt, Filter.of(2))
        assertEquals(Filter.None, Filter.of(99))
        assertEquals(Filter.None, Filter.of(-1))
    }

    @Test fun onlyTheLooksThatNeedAnOffScreenPassAskForOne() {
        // An off-screen pass is extra draws, so it runs only where it earns them.
        // Everything else is still one shader with a branch.
        assertEquals(
            listOf(Filter.Cartoon, Filter.Smooth),
            Filter.entries.filter { it.source == Source.Smoothed },
        )
        assertEquals(listOf(Filter.Composite), Filter.entries.filter { it.source == Source.Composite })
        assertEquals(Source.Direct, Filter.None.source)
    }

    @Test fun theNewIdsFollowOnWithoutReusingTheRetiredOne() {
        assertEquals(9, Filter.Smooth.id)
        assertEquals(10, Filter.Composite.id)
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
