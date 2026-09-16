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

class PerformanceUiTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private fun awaitText(text: String) = compose.waitUntil(15_000) {
        compose.onAllNodesWithText(text).fetchSemanticsNodes(atLeastOneRootRequired = false).isNotEmpty()
    }

    @Test fun rewindSwitchStopsRecordingAndPersistsAcrossRecreation() {
        val preferences = context.getSharedPreferences("settings", Context.MODE_PRIVATE)
        val existed = preferences.contains("rewindHistory")
        val original = preferences.getBoolean("rewindHistory", true)
        // A synthetic cartridge exists only in native memory; no library or saves touched.
        val rom = ByteArray(16 + 16384).apply {
            byteArrayOf(0x4e, 0x45, 0x53, 0x1a).copyInto(this)
            this[4] = 1
            byteArrayOf(0x4c, 0, 0x80.toByte()).copyInto(this, 16)
            this[16 + 0x3ffd] = 0x80.toByte()
        }
        val pixels = ByteBuffer.allocateDirect(256 * 240 * 4)
        try {
            preferences.edit().putBoolean("rewindHistory", true).commit()
            Native.setRewindEnabled(true)
            Native.load(rom)
            repeat(2) { Native.frame(pixels, 0, 0, true) }
            assertEquals(2, Native.rewindDepth())
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                awaitText("Settings")
                compose.onNodeWithText("Settings").performClick()
                compose.onNodeWithText("Rewind history").performScrollTo().assertIsOn().performClick()
                compose.onNodeWithText("Rewind history").assertIsOff()
                compose.waitUntil(15_000) { Native.rewindDepth() == 0 }
                repeat(2) { Native.frame(pixels, 0, 0, true) }
                assertEquals(0, Native.rewindDepth())
                assertFalse(Settings(context).rewindHistory)
                compose.onNodeWithText("Audio gaps").performScrollTo()
                compose.onNodeWithText("Paused").assertIsDisplayed()
                compose.onRoot().captureToImage().asAndroidBitmap().let { bitmap ->
                    File(context.cacheDir, "performance-settings.png").outputStream().use {
                        bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
                    }
                }
                scenario.recreate()
                awaitText("Settings")
                compose.onNodeWithText("Settings").performClick()
                compose.onNodeWithText("Rewind history").performScrollTo().assertIsOff()
                Native.load(rom)
                Native.frame(pixels, 0, 0, true)
                assertEquals(0, Native.rewindDepth())
                compose.onNodeWithText("Rewind history").performClick()
                compose.waitUntil(15_000) {
                    Native.frame(pixels, 0, 0, true)
                    Native.rewindDepth() > 0
                }
                assertTrue(Settings(context).rewindHistory)
            }
        } finally {
            val edit = preferences.edit()
            if (existed) edit.putBoolean("rewindHistory", original) else edit.remove("rewindHistory")
            edit.commit()
            Native.setRewindEnabled(original)
        }
    }
}
