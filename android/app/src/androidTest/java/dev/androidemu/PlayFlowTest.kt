package dev.androidemu

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.nio.ByteBuffer

class PlayFlowTest {
    @get:Rule val compose = createEmptyComposeRule()
    private fun rom(): ByteArray {
        val rom = ByteArray(16 + 16384)
        byteArrayOf(0x4e, 0x45, 0x53, 0x1a).copyInto(rom)
        rom[4] = 1; rom[6] = 2
        // Infinite JMP loop with a valid reset vector; CHR RAM and battery RAM.
        byteArrayOf(0x4c, 0, 0x80.toByte()).copyInto(rom, 16)
        rom[16 + 0x3ffd] = 0x80.toByte()
        return rom
    }
    private val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private fun showing(text: String) = compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()
    private fun awaitText(text: String) = compose.waitUntil(60_000) { showing(text) }

    @Test fun nativeStateRoundTripAndRejectedCorruption() {
        Native.load(rom())
        val pixels = ByteBuffer.allocateDirect(256 * 240 * 4)
        Native.frame(pixels, 0, 0, true)
        val saved = Native.snapshot(false)
        Native.frame(pixels, 1, 0, true)
        val expected = Native.snapshot(false)
        Native.restore(saved, false)
        Native.frame(pixels, 1, 0, true)
        assertArrayEquals(expected, Native.snapshot(false))
        val bad = saved.clone(); bad[30] = (bad[30].toInt() xor 1).toByte()
        try { Native.restore(bad, false); fail("Corrupt state was accepted") } catch (_: IllegalStateException) {}
        assertArrayEquals(expected, Native.snapshot(false))
    }

    /** Play, pause, save a slot, load it back, background, and resume. */
    @Test fun playPauseSaveLoadAndResume() {
        val library = Library(context)
        val bytes = rom(); val id = Native.load(bytes)
        library.add(id, "Test Adventure", bytes)
        val game = Game(id, "Test Adventure", 0)
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            awaitText("Test Adventure")
            compose.onNodeWithText("Test Adventure").performClick()
            // Opening a game resumes it, so the in-game bar is what appears next.
            awaitText("Menu")
            compose.onNodeWithText("Menu").performClick()
            awaitText("Take your time")
            compose.waitUntil(60_000) { library.state(game, -1).exists() }
            compose.onNodeWithText("Save states · 10 slots").performClick()
            compose.onAllNodesWithText("Save")[0].performClick()
            awaitText("Saved to slot 1.")
            compose.onNodeWithText("Got it").performClick()
            compose.onAllNodesWithText("Load")[0].performClick()
            awaitText("Save loaded. Tap Resume when you're ready.")
            compose.onNodeWithText("Got it").performClick()
            compose.onRoot().captureToImage().asAndroidBitmap().let { bitmap ->
                File(context.cacheDir, "pause-smoke.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            }
            compose.onNodeWithText("Resume game").performClick()
            scenario.moveToState(androidx.lifecycle.Lifecycle.State.CREATED)
            assertTrue(library.state(game, -1).exists())
            scenario.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED)
            compose.onNodeWithText("Take your time").assertIsDisplayed()
        }
    }

    /** Picture and control settings are reachable from the shelf and stick. */
    @Test fun settingsCycleAndPersist() {
        val settings = Settings(context)
        settings.aspect = Aspect.Television
        settings.scanlines = false
        ActivityScenario.launch(MainActivity::class.java).use {
            awaitText("Your next adventure")
            compose.onNodeWithText("Settings").performClick()
            awaitText("Shape")
            compose.onNodeWithText(Aspect.Television.label).performClick()
            compose.waitUntil(10_000) { showing(Aspect.Hardware.label) }
            compose.onNodeWithText("Scanlines").performClick()
            compose.waitUntil(10_000) { compose.onAllNodesWithText("On").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Done").performClick()
        }
        // A new reader sees what the panel wrote, not the value it started with.
        val reloaded = Settings(context)
        assertEquals(Aspect.Hardware, reloaded.aspect)
        assertTrue(reloaded.scanlines)
        reloaded.aspect = Aspect.Television
        reloaded.scanlines = false
    }

    /** Box art outranks the screenshot, and removing it falls back rather than blanks. */
    @Test fun chosenBoxArtOutranksTheSavedScreenshot() {
        val library = Library(context)
        val bytes = rom(); val id = Native.load(bytes)
        val game = library.add(id, "Cover Test", bytes)
        val art = library.art(game)
        val thumbnail = library.thumbnail(game, -1)
        art.delete(); thumbnail.delete()
        assertNull(library.cover(game))
        Library.atomic(thumbnail, byteArrayOf(1))
        assertEquals(thumbnail, library.cover(game))
        Library.atomic(art, byteArrayOf(2))
        assertEquals(art, library.cover(game))
        assertTrue(art.delete())
        assertEquals(thumbnail, library.cover(game))
    }

    /** Playtime accumulates and the most recently played game leads the shelf. */
    @Test fun playtimeAccumulatesAndOrdersTheShelf() {
        val library = Library(context)
        val first = library.add(Native.load(rom()), "Older", rom())
        // A second game with a different payload, so it hashes to its own id.
        val other = rom().also { it[16 + 4] = 0x2a }
        val second = library.add(Native.load(other), "Newer", other)
        library.record(first, 120)
        library.record(second, 30)
        val games = library.games()
        assertEquals("the most recently played comes first", "Newer", games.first().title)
        assertEquals(120, games.first { it.id == first.id }.seconds)
        library.record(first, 60)
        assertEquals("Older", library.games().first().title)
        assertEquals(180, library.games().first { it.id == first.id }.seconds)
    }
}
