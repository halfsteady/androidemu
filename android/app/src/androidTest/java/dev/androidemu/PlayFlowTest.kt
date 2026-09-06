package dev.androidemu

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
    @Test fun libraryPlayPauseSaveLoadAndResume() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val library = Library(context)
        val bytes = rom(); val id = Native.load(bytes)
        library.add(id, "Test Adventure", bytes)
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            compose.waitUntil(60_000) { compose.onAllNodesWithText("Test Adventure").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Test Adventure").performClick()
            compose.waitUntil(60_000) { compose.onAllNodesWithText("Pause & save").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Pause & save").performClick()
            compose.waitUntil(60_000) { library.state(Game(id, "Test Adventure", 0), -1).exists() }
            compose.onNodeWithText("Take your time").assertIsDisplayed()
            compose.onNodeWithText("Save & load moments").performClick()
            compose.onAllNodesWithText("Save")[0].performClick()
            compose.waitUntil(60_000) { compose.onAllNodesWithText("Saved to slot 1.").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Got it").performClick()
            compose.onAllNodesWithText("Load")[0].performClick()
            compose.waitUntil(60_000) { compose.onAllNodesWithText("Save loaded. Tap Resume when you're ready.").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Got it").performClick()
            compose.onRoot().captureToImage().asAndroidBitmap().let { bitmap ->
                File(context.cacheDir, "pause-smoke.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            }
            compose.onNodeWithText("Resume game").performClick()
            scenario.moveToState(androidx.lifecycle.Lifecycle.State.CREATED)
            assertTrue(library.state(Game(id, "Test Adventure", 0), -1).exists())
            scenario.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED)
            compose.onNodeWithText("Take your time").assertIsDisplayed()
        }
    }
}
