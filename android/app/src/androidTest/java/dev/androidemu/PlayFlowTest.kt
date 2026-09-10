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
    private fun showing(text: String, substring: Boolean = false) =
        compose.onAllNodesWithText(text, substring = substring).fetchSemanticsNodes(atLeastOneRootRequired = false).isNotEmpty()
    private fun awaitText(text: String, substring: Boolean = false) =
        compose.waitUntil(60_000) { showing(text, substring) }

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
            compose.onNodeWithText("Save states").performClick()
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

    /** Reset replaces only the live session/autosave, with cancellation before it. */
    @Test fun resetKeepsBatteryAndManualSavesAndReplacesAutosave() {
        val library = Library(context)
        val bytes = rom().apply { this[31] = 0x7c }
        val id = Native.load(bytes)
        val game = library.add(id, "Reset Test", bytes)
        val battery = Native.snapshot(true).apply { this[100] = 42 }
        Native.restore(battery, true)
        val fresh = Native.snapshot(false)
        val pixels = ByteBuffer.allocateDirect(256 * 240 * 4)
        repeat(6) { Native.frame(pixels, 0, 0, true) }
        val manual = Native.snapshot(false)
        Library.atomic(library.battery(game), battery)
        Library.atomic(library.state(game, 0), manual)
        Library.atomic(library.state(game, -1), manual)

        ActivityScenario.launch(MainActivity::class.java).use {
            awaitText("Reset Test")
            compose.onNodeWithText("Reset Test").performClick()
            awaitText("Menu")
            compose.onNodeWithText("Menu").performClick()
            compose.waitUntil(60_000) { runCatching { compose.onNodeWithText("Reset game").assertIsEnabled() }.isSuccess }
            val beforeCancel = library.state(game, -1).readBytes()
            compose.onNodeWithText("Reset game").performClick()
            awaitText("Reset game?")
            compose.onNodeWithText("Cancel").performClick()
            assertArrayEquals(beforeCancel, library.state(game, -1).readBytes())

            compose.onNodeWithText("Reset game").performClick()
            compose.onNodeWithText("Reset").performClick()
            compose.waitUntil(60_000) { library.state(game, -1).readBytes().contentEquals(fresh) }
            compose.waitUntil(10_000) { !showing("Take your time") }
            assertArrayEquals(battery, library.battery(game).readBytes())
            assertArrayEquals(manual, library.state(game, 0).readBytes())
            assertTrue(showing("Menu"))
        }
    }

    /** Picture settings preview, apply and persist. */
    @Test fun pictureSettingsPreviewAndPersist() {
        val settings = Settings(context)
        settings.aspect = Aspect.Television
        settings.filter = Filter.None
        settings.trimEdges = false
        ActivityScenario.launch(MainActivity::class.java).use {
            awaitText("Your next adventure")
            compose.onNodeWithText("Settings").performClick()
            awaitText("Shape")
            // The preview is drawn by the GL thread, so the placeholder has to go.
            compose.waitUntil(30_000) { compose.onAllNodesWithText("Preparing a preview…").fetchSemanticsNodes().isEmpty() }
            compose.onNodeWithContentDescription("Preview of the current picture settings").assertIsDisplayed()
            // Chips, so any shape or look is one tap from the preview.
            compose.onNodeWithText(Aspect.Hardware.label).performClick()
            compose.onNodeWithText(Filter.GameBoy.label).performClick()
            compose.waitUntil(10_000) { showing(Filter.GameBoy.note) }
            compose.onNodeWithText("Trim the edges").performClick()
            compose.onNodeWithText("Done").performClick()
        }
        // A new reader sees what the panel wrote, not the value it started with.
        val reloaded = Settings(context)
        assertEquals(Aspect.Hardware, reloaded.aspect)
        assertEquals(Filter.GameBoy, reloaded.filter)
        assertTrue(reloaded.trimEdges)
        reloaded.aspect = Aspect.Television
        reloaded.filter = Filter.None
        reloaded.trimEdges = false
    }

    /** The scanlines switch the first release shipped becomes the scanlines look. */
    @Test fun anOldScanlinesSwitchBecomesTheScanlinesLook() {
        val preferences = context.getSharedPreferences("settings", Context.MODE_PRIVATE)
        preferences.edit().clear().putBoolean("scanlines", true).commit()
        assertEquals(Filter.Scanlines, Settings(context).filter)
        // Once a look is chosen explicitly, the old switch stops speaking for it.
        Settings(context).filter = Filter.Sepia
        assertEquals(Filter.Sepia, Settings(context).filter)
        preferences.edit().clear().commit()
        assertEquals(Filter.None, Settings(context).filter)
    }

    /** The skip-back buttons only offer what the rewind chain can actually honour. */
    @Test fun skipBackIsOfferedOnlyWhenThereIsSomethingToGoBackTo() {
        val library = Library(context)
        val bytes = rom(); val id = Native.load(bytes)
        library.add(id, "Skip Test", bytes)
        ActivityScenario.launch(MainActivity::class.java).use {
            awaitText("Skip Test")
            compose.onNodeWithText("Skip Test").performClick()
            awaitText("Menu")
            val back5 = compose.onNodeWithContentDescription("Back 5 seconds")
            // A fresh game has no history, so the jump is inert until it does.
            back5.assertIsNotEnabled()
            compose.waitUntil(60_000) { runCatching { back5.assertIsEnabled() }.isSuccess }
            back5.performClick()
            compose.onNodeWithContentDescription(timeControl).assertIsDisplayed()
        }
    }

    private val timeControl =
        "Time control. Drag left to rewind, right to fast-forward — the further from the middle, the faster. Tap the handle to pause."

    /**
     * The handle carries a pause glyph, so tapping it has to pause. It read as a
     * pause button before it was one, which is the bug this pins shut.
     */
    @Test fun tappingTheTimeControlHandlePauses() {
        val library = Library(context)
        val bytes = rom(); val id = Native.load(bytes)
        library.add(id, "Handle Test", bytes)
        ActivityScenario.launch(MainActivity::class.java).use {
            awaitText("Handle Test")
            compose.onNodeWithText("Handle Test").performClick()
            // Opening a game resumes it, so the pause panel is not up yet.
            awaitText("Menu")
            compose.onAllNodesWithText("Take your time").assertCountEquals(0)
            // A tap lands at the node centre, which at rest is the handle.
            compose.onNodeWithContentDescription(timeControl).performTouchInput { click() }
            awaitText("Take your time")
        }
    }

    /** In full screen the picture is the control: a tap shows the chrome, a tap hides it. */
    @Test fun tappingTheScreenTogglesTheFullScreenChrome() {
        val library = Library(context)
        val bytes = rom(); val id = Native.load(bytes)
        library.add(id, "Chrome Test", bytes)
        ActivityScenario.launch(MainActivity::class.java).use {
            awaitText("Chrome Test")
            compose.onNodeWithText("Chrome Test").performClick()
            awaitText("Full screen")
            compose.onNodeWithText("Full screen").performClick()
            // The full-screen chrome starts up, so there is something to hide.
            awaitText("Exit full screen")
            compose.onNodeWithContentDescription(timeControl).assertIsDisplayed()
            // The tap layer sits over the picture, which is the centre of the root.
            compose.onRoot().performTouchInput { click() }
            compose.waitUntil(10_000) { !showing("Exit full screen") }
            compose.onRoot().performTouchInput { click() }
            awaitText("Exit full screen")
        }
    }

    /**
     * Putting a game away takes it off the shelf and loses nothing; bringing it
     * back returns it mid-adventure. Deleting is the only thing that removes a
     * save, and it lives behind its own confirmation.
     */
    @Test fun puttingAGameAwayKeepsEverythingAndBringingItBackRestoresIt() {
        val library = Library(context)
        val bytes = rom(); val id = Native.load(bytes)
        val game = library.add(id, "Archive Test", bytes)
        library.setArchived(game, false)
        // A save state, to prove archiving does not touch what is on disk.
        Library.atomic(library.state(game, 3), byteArrayOf(1, 2, 3))

        assertTrue(library.games().any { it.id == id })
        assertFalse(library.archived().any { it.id == id })

        library.setArchived(game, true)
        assertFalse("an archived game is off the shelf", library.games().any { it.id == id })
        assertTrue(library.archived().any { it.id == id })
        assertTrue("archiving must not delete a save", library.state(game, 3).exists())
        assertTrue(File(library.directory(id), "game.nes").exists())

        library.setArchived(game, false)
        assertTrue(library.games().any { it.id == id })
        assertTrue(library.state(game, 3).exists())

        // Deleting is the one that does take it all.
        library.forget(game)
        assertFalse(library.games().any { it.id == id })
        assertFalse(library.archived().any { it.id == id })
        assertFalse(File(library.directory(id), "game.nes").exists())
    }

    /** The shelf puts a game away and brings it back, and only offers the cupboard when it holds something. */
    @Test fun theShelfPutsAGameAwayAndBringsItBack() {
        val library = Library(context)
        library.archived().forEach { library.setArchived(it, false) }
        val bytes = rom(); val id = Native.load(bytes)
        library.add(id, "Shelf Archive", bytes)
        ActivityScenario.launch(MainActivity::class.java).use {
            awaitText("Shelf Archive")
            compose.onNodeWithContentDescription("More for Shelf Archive").performClick()
            awaitText("Put this away")
            compose.onNodeWithText("Put this away").performClick()
            awaitText("is put away.", substring = true)
            compose.onNodeWithText("Got it").performClick()
            compose.waitUntil(10_000) { !showing("Shelf Archive") }
            compose.onNodeWithText("Put away (1)").performClick()
            awaitText("Bring back")
            compose.onNodeWithText("Bring back").performClick()
            awaitText("is back on the shelf", substring = true)
            compose.onNodeWithText("Got it").performClick()
            awaitText("Shelf Archive")
        }
        library.games().firstOrNull { it.id == id }?.let { library.forget(it) }
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
