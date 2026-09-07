package dev.androidemu

import org.junit.Assert.*
import org.junit.Test

/**
 * The palette model, checked against the table the core ships.
 *
 * The point of generating colours from the 2C02's own behaviour rather than
 * pasting a table is that the result can be checked. If the model drifts, these
 * fail rather than the picture quietly restaining itself.
 */
class PaletteTest {
    /**
     * The core's table, from `native/src/lib.rs`. Duplicated here on purpose: it
     * is the thing being checked against, so a copy that cannot silently follow a
     * change on the other side is what makes the check worth running.
     */
    private val core = intArrayOf(
        0x666666, 0x002a88, 0x1412a7, 0x3b00a4, 0x5c007e, 0x6e0040, 0x6c0600, 0x561d00,
        0x333500, 0x0b4800, 0x005200, 0x004f08, 0x00404d, 0, 0, 0,
        0xadadad, 0x155fd9, 0x4240ff, 0x7527fe, 0xa01acc, 0xb71e7b, 0xb53120, 0x994e00,
        0x6b6d00, 0x388700, 0x0c9300, 0x008f32, 0x007c8d, 0, 0, 0,
        0xfffeff, 0x64b0ff, 0x9290ff, 0xc676ff, 0xf36aff, 0xfe6ecc, 0xfe8170, 0xea9e22,
        0xbcbe00, 0x88d800, 0x5ce430, 0x45e082, 0x48cdde, 0x4f4f4f, 0, 0,
        0xfffeff, 0xc0dfff, 0xd3d2ff, 0xe8c8ff, 0xfbc2ff, 0xfec4ea, 0xfeccc5, 0xf7d8a5,
        0xe4e594, 0xcfef96, 0xbdf4ab, 0xb3f3cc, 0xb5ebf2, 0xb8b8b8, 0, 0,
    )

    private fun channels(colour: Int) = intArrayOf((colour shr 16) and 255, (colour shr 8) and 255, colour and 255)

    private fun distance(a: Int, b: Int): Int {
        val (ar, ag, ab) = channels(a).toList()
        val (br, bg, bb) = channels(b).toList()
        return maxOf(kotlin.math.abs(ar - br), kotlin.math.abs(ag - bg), kotlin.math.abs(ab - bb))
    }

    @Test fun theModelReproducesTheTableTheCoreShips() {
        val model = PaletteModel.standard()
        val errors = mutableListOf<Int>()
        for (index in 0 until 64) {
            // $xE and $xF are blanking, and are black in every palette.
            if (index and 15 >= 14) continue
            // The unused slots the core leaves at zero prove nothing either way.
            if (core[index] == 0 && (index and 15) != 13) continue
            errors += distance(model[index], core[index])
        }
        val mean = errors.average()
        assertTrue("only ${errors.size} colours compared", errors.size >= 50)
        assertTrue("mean error $mean is too far from the core's table", mean < 15.0)
        assertTrue("worst error ${errors.max()} is too far", errors.max() < 40)
    }

    @Test fun everyColourIsInRange() {
        for (palette in Palette.entries) {
            val colours = palette.colours() ?: continue
            assertEquals(palette.name, 64, colours.size)
            for (colour in colours) {
                assertEquals("$palette has a colour outside 24 bits", 0, colour and 0xff000000.toInt())
            }
        }
    }

    @Test fun theBlankingColoursAreNeverBrighterThanTheDarkestRealOne() {
        // $xE and $xF are the PPU blanking. Brightness is allowed to lift them,
        // because that is what a television's brightness control did to black —
        // but nothing may make them brighter than a colour the game can draw.
        fun luma(c: Int) = 0.299 * ((c shr 16) and 255) + 0.587 * ((c shr 8) and 255) + 0.114 * (c and 255)
        for (knobs in listOf(
            PaletteModel.build(),
            PaletteModel.build(saturation = 2.0, contrast = 1.5, brightness = 0.2),
            PaletteModel.standard(),
        )) {
            val blanking = (0 until 64).filter { it and 15 >= 14 }.map { luma(knobs[it]) }
            val real = (0 until 64).filter { it and 15 < 13 }.map { luma(knobs[it]) }
            assertTrue("blanking is not uniform", blanking.toSet().size == 1)
            assertTrue("blanking outshines a real colour", blanking.max() <= real.min() + 0.001)
        }
        // With the knobs where they ship, it is simply black.
        for (level in 0 until 4) for (hue in 14..15) {
            assertEquals("$${(level shl 4 or hue).toString(16)}", 0, PaletteModel.build()[level shl 4 or hue])
        }
    }

    @Test fun turningTheColourDownLeavesGreys() {
        val grey = PaletteModel.build(saturation = 0.0)
        for (index in 0 until 64) {
            val (r, g, b) = channels(grey[index]).toList()
            assertTrue("$$index is not grey: $r $g $b", maxOf(r, g, b) - minOf(r, g, b) <= 2)
        }
    }

    @Test fun aPaletteSurvivesTheRoundTripToBytesAndBack() {
        val original = PaletteModel.build(saturation = 1.2, tint = 0.5)
        val back = PaletteModel.parse(PaletteModel.bytes(original))
        assertArrayEquals(original, back)
    }

    @Test fun aFileTooShortToBeAPaletteIsRefused() {
        assertNull(PaletteModel.parse(ByteArray(191)))
        assertNotNull(PaletteModel.parse(ByteArray(192)))
        // The published files carry the emphasis variants after the first 64.
        assertNotNull(PaletteModel.parse(ByteArray(64 * 3 * 8)))
    }

    @Test fun theIdsAreTheSavedValuesSoTheyMustNotMove() {
        assertEquals(0, Palette.Standard.id)
        assertEquals(5, Palette.File.id)
        // 4 was "Cool", cut. A saved 4 falls back rather than landing on File.
        assertFalse("id 4 is retired", Palette.entries.any { it.id == 4 })
        assertEquals(Palette.Standard, Palette.of(4))
        assertEquals("ids must be unique", Palette.entries.size, Palette.entries.map { it.id }.distinct().size)
        assertEquals(Palette.Standard, Palette.of(99))
    }

    @Test fun everyPaletteIsNamedAndExplained() {
        for (palette in Palette.entries) {
            assertTrue(palette.name, palette.label.isNotBlank())
            assertTrue(palette.name, palette.note.isNotBlank())
        }
        assertEquals("no two share a name", Palette.entries.size, Palette.entries.map { it.label }.distinct().size)
    }
}
