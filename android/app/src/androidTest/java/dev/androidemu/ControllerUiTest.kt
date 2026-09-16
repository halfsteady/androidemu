package dev.androidemu

import android.content.Context
import android.graphics.Bitmap
import android.os.SystemClock
import android.view.InputDevice
import android.view.KeyEvent
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeNotNull
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Real activity/window dispatch with the Pi's connected keyboard and adapter.
 * No ROM is opened. Controller preferences are restored even on failure. */
class ControllerUiTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private fun awaitText(text: String) = compose.waitUntil(15_000) {
        compose.onAllNodesWithText(text).fetchSemanticsNodes(atLeastOneRootRequired = false).isNotEmpty()
    }
    private fun capture(name: String) {
        compose.onRoot().captureToImage().asAndroidBitmap().let { bitmap ->
            File(context.cacheDir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
    private fun press(scenario: ActivityScenario<MainActivity>, id: Int, code: Int, scan: Int = 0) {
        scenario.onActivity { activity ->
            val now = SystemClock.uptimeMillis()
            for (action in listOf(KeyEvent.ACTION_DOWN, KeyEvent.ACTION_UP)) {
                activity.dispatchKeyEvent(KeyEvent(now, now, action, code, 0, 0, id, scan, 0, InputDevice.SOURCE_KEYBOARD))
            }
        }
    }

    @Test fun startCannotResumeBehindControllerSetupAndStillResumesThePauseMenu() {
        // A temporary cartridge loops forever. The user's cartridges/saves are never opened.
        val rom = ByteArray(16 + 16384)
        byteArrayOf(0x4e, 0x45, 0x53, 0x1a).copyInto(rom)
        rom[4] = 1
        byteArrayOf(0x4c, 0, 0x80.toByte()).copyInto(rom, 16)
        "controller-menu-regression".toByteArray().copyInto(rom, 32)
        rom[16 + 0x3ffd] = 0x80.toByte()
        val library = Library(context)
        val id = Native.load(rom)
        check(library.games().none { it.id == id } && library.archived().none { it.id == id })
        val game = library.add(id, "Controller Menu Regression", rom)
        try {
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                awaitText(game.title)
                compose.onNodeWithText(game.title).performScrollTo().performClick()
                awaitText("Menu")
                compose.onNodeWithText("Menu").performClick()
                awaitText("Take your time")
                compose.waitUntil(15_000) {
                    runCatching { compose.onNodeWithText("Controllers").assertIsEnabled() }.isSuccess
                }
                compose.onNodeWithText("Controllers").performScrollTo().performClick()
                awaitText("Connected devices".uppercase())
                press(scenario, -1, KeyEvent.KEYCODE_BUTTON_START)
                compose.onNodeWithText("Controllers").assertIsDisplayed()
                compose.onNodeWithText("Done").performScrollTo().performClick()
                awaitText("Take your time")
                press(scenario, -1, KeyEvent.KEYCODE_BUTTON_START)
                compose.onAllNodesWithText("Take your time").assertCountEquals(0)
                compose.onNodeWithText("Menu").assertIsDisplayed()
                compose.onNodeWithText("Menu").performClick()
                awaitText("Take your time")
            }
        } finally {
            library.forget(game)
        }
    }

    @Test fun chooseDeviceAssignPlayerAndMapOnlyThatDevice() {
        val devices = InputDevice.getDeviceIds().map { InputDevice.getDevice(it) }.filterNotNull()
        val pad = devices.firstOrNull { it.vendorId == 0x0810 && it.productId == 0xe501 }
        val keyboard = devices.firstOrNull { it.name == "K830 Keyboard" }
        assumeNotNull(pad, keyboard)
        val preferences = context.getSharedPreferences("controllers", Context.MODE_PRIVATE)
        val before = preferences.all.toMap()
        try {
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                awaitText("Settings")
                compose.onNodeWithText("Settings").performClick()
                compose.onNodeWithText("Controllers").performScrollTo().performClick()
                awaitText("Player 2")
                capture("controllers-list.png")
                compose.onNode(hasText(pad!!.name.trim()) and hasClickAction()).performScrollTo().performClick()
                compose.onNodeWithText("Player 2").performClick()
                compose.onNodeWithText("Player 2 · Gamepad").assertIsDisplayed()
                capture("controller-details.png")
                compose.onNodeWithText("Set buttons").performScrollTo().performClick()
                awaitText("Set buttons · Player 2")
                compose.onNodeWithText(pad.name.trim()).assertIsDisplayed()
                press(scenario, keyboard!!.id, KeyEvent.KEYCODE_X, 45)
                compose.onNodeWithText("Press  A").assertIsDisplayed()
                press(scenario, pad.id, KeyEvent.KEYCODE_BUTTON_2, 289)
                compose.onNodeWithText("Press  B").assertIsDisplayed()
                // A duplicate and reserved shoulder must not advance the wizard.
                press(scenario, pad.id, KeyEvent.KEYCODE_BUTTON_2, 289)
                press(scenario, pad.id, KeyEvent.KEYCODE_BUTTON_R1)
                compose.onNodeWithText("Press  B").assertIsDisplayed()
                press(scenario, pad.id, KeyEvent.KEYCODE_BUTTON_1, 288)
                press(scenario, pad.id, KeyEvent.KEYCODE_BUTTON_9, 296)
                compose.onNodeWithText("Press  Start").assertIsDisplayed()
                capture("controller-mapping.png")
                press(scenario, pad.id, KeyEvent.KEYCODE_BUTTON_10, 297)
                awaitText("Buttons saved for ${pad.name.trim()} · Player 2.")
                compose.onNodeWithText("Player 2 · Gamepad").assertIsDisplayed()
                // Completing Start's release must not activate Set buttons again.
                compose.onAllNodesWithText("Press  A").assertCountEquals(0)
                scenario.recreate()
                awaitText("Settings")
                compose.onNodeWithText("Settings").performClick()
                compose.onNodeWithText("Controllers").performScrollTo().performClick()
                compose.onNode(hasText(pad.name.trim()) and hasClickAction()).performScrollTo().performClick()
                compose.onNodeWithText("Player 2 · Gamepad").assertIsDisplayed()
                // Cancel leaves the completed mapping intact.
                val completed = preferences.all.toMap()
                compose.onNodeWithText("Set buttons").performScrollTo().performClick()
                press(scenario, pad.id, KeyEvent.KEYCODE_BUTTON_1, 288)
                press(scenario, keyboard.id, KeyEvent.KEYCODE_ESCAPE)
                awaitText("Player 2 · Gamepad")
                assertEquals(completed, preferences.all)
            }
        } finally {
            val edit = preferences.edit().clear()
            before.forEach { (key, value) ->
                when (value) {
                    is Int -> edit.putInt(key, value)
                    is String -> edit.putString(key, value)
                    is Boolean -> edit.putBoolean(key, value)
                    is Long -> edit.putLong(key, value)
                    is Float -> edit.putFloat(key, value)
                    is Set<*> -> edit.putStringSet(key, value.filterIsInstance<String>().toSet())
                }
            }
            edit.commit()
            assertEquals(before, preferences.all)
        }
    }
}
