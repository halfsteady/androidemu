package dev.androidemu

import android.content.Context
import android.content.ContextWrapper
import android.content.SharedPreferences
import android.os.SystemClock
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.*
import org.junit.Assume.assumeNotNull
import org.junit.Before
import org.junit.Test

/** Exercises the real Android input adapter without injecting keys into the UI.
 * Preferences are isolated from the installed app's controller mappings. */
class ControllerInputTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val preferencesName = "controller-input-regression"
    private lateinit var input: ControllerInput
    private var disconnects = 0
    private val testContext = object : ContextWrapper(context) {
        override fun getSharedPreferences(name: String, mode: Int): SharedPreferences =
            super.getSharedPreferences(preferencesName, mode)
    }

    @Before fun setUp() {
        context.getSharedPreferences(preferencesName, Context.MODE_PRIVATE).edit().clear().commit()
        instrumentation.runOnMainSync { input = ControllerInput(testContext) { disconnects++ } }
    }

    @After fun tearDown() {
        instrumentation.runOnMainSync { input.close() }
        context.deleteSharedPreferences(preferencesName)
    }

    private fun key(id: Int, code: Int, action: Int = KeyEvent.ACTION_DOWN,
                    source: Int = InputDevice.SOURCE_GAMEPAD, scan: Int = 0): KeyEvent {
        val now = SystemClock.uptimeMillis()
        return KeyEvent(now, now, action, code, 0, 0, id, scan, 0, source)
    }

    private fun motion(id: Int, x: Float): MotionEvent {
        val now = SystemClock.uptimeMillis()
        val properties = MotionEvent.PointerProperties().apply { this.id = 0 }
        val coords = MotionEvent.PointerCoords().apply { setAxisValue(MotionEvent.AXIS_X, x) }
        return MotionEvent.obtain(now, now, MotionEvent.ACTION_MOVE, 1, arrayOf(properties),
            arrayOf(coords), 0, 0, 1f, 1f, id, 0, InputDevice.SOURCE_JOYSTICK, 0)
    }

    @Test fun keyboardThenGamepadBothControlPlayerOne() {
        assertTrue(input.key(key(-1, KeyEvent.KEYCODE_ENTER, source = InputDevice.SOURCE_KEYBOARD)))
        assertEquals(8 to 0, input.buttons())
        input.clear() // Mapping/pausing clears held input but used to retain the keyboard's port.
        assertTrue(input.key(key(10001, KeyEvent.KEYCODE_BUTTON_START)))
        assertEquals(8 to 0, input.buttons())
        input.key(key(10001, KeyEvent.KEYCODE_BUTTON_START, KeyEvent.ACTION_UP))
        assertEquals(0 to 0, input.buttons())
    }

    @Test fun keyboardDoesNotTakeEitherOfTwoGamepadPorts() {
        input.key(key(-1, KeyEvent.KEYCODE_X, source = InputDevice.SOURCE_KEYBOARD))
        assertTrue(input.key(key(10001, KeyEvent.KEYCODE_BUTTON_A)))
        assertTrue(input.key(key(10002, KeyEvent.KEYCODE_BUTTON_START)))
        assertEquals(3 to 8, input.buttons())
        assertFalse(input.key(key(10003, KeyEvent.KEYCODE_BUTTON_B)))
        assertEquals(3 to 8, input.buttons())
    }

    @Test fun releaseWithoutPressDoesNotReserveAPlayerPort() {
        input.key(key(10001, KeyEvent.KEYCODE_BUTTON_START, KeyEvent.ACTION_UP))
        input.key(key(10002, KeyEvent.KEYCODE_BUTTON_B))
        assertEquals(1 to 0, input.buttons())
    }

    @Test fun neutralMotionDoesNotReserveAPlayerPort() {
        val centered = motion(10001, 0f)
        try { input.motion(centered) } finally { centered.recycle() }
        input.key(key(10002, KeyEvent.KEYCODE_BUTTON_B))
        assertEquals(1 to 0, input.buttons())
    }

    @Test fun directionsReleaseAndClearWithoutMovingTheSecondController() {
        val right = motion(10001, 1f)
        try { assertTrue(input.motion(right)) } finally { right.recycle() }
        input.key(key(10002, KeyEvent.KEYCODE_BUTTON_START))
        assertEquals(128 to 8, input.buttons())
        val centered = motion(10001, 0f)
        try { input.motion(centered) } finally { centered.recycle() }
        assertEquals(0 to 8, input.buttons())
        input.clear()
        assertEquals(0 to 0, input.buttons())
        input.key(key(10002, KeyEvent.KEYCODE_BUTTON_B))
        assertEquals(0 to 1, input.buttons())
    }

    @Test fun disconnectReleasesButtonsAndReplacementGetsTheVacantPort() {
        input.key(key(10001, KeyEvent.KEYCODE_BUTTON_B))
        input.key(key(10002, KeyEvent.KEYCODE_BUTTON_A))
        input.onInputDeviceRemoved(10001)
        assertEquals(1, disconnects)
        assertEquals(0 to 2, input.buttons())
        input.key(key(10003, KeyEvent.KEYCODE_BUTTON_START))
        assertEquals(8 to 2, input.buttons())
    }

    @Test fun mappingSurvivesRecreationAndStartPressReleaseRepeat() {
        val start = key(10001, KeyEvent.KEYCODE_BUTTON_10, scan = 297)
        input.saveMapping(listOf(start to 8))
        instrumentation.runOnMainSync { input.close(); input = ControllerInput(testContext) {} }
        repeat(3) {
            assertEquals(8, input.bitFor(start)) // The activity uses this to resume.
            assertTrue(input.key(start))
            assertEquals(8 to 0, input.buttons())
            assertTrue(input.key(key(10001, KeyEvent.KEYCODE_BUTTON_10, KeyEvent.ACTION_UP, scan = 297)))
            assertEquals(0 to 0, input.buttons())
        }
    }

    private fun connectedPad(): InputDevice {
        val pad = InputDevice.getDeviceIds().map { InputDevice.getDevice(it) }.filterNotNull()
            .firstOrNull { it.supportsSource(InputDevice.SOURCE_GAMEPAD) ||
                it.getMotionRange(MotionEvent.AXIS_X, InputDevice.SOURCE_JOYSTICK) != null }
        assumeNotNull(pad)
        return pad!!
    }

    @Test fun chosenPlayerPersistsThroughRemappingRecreationAndReconnect() {
        val pad = connectedPad()
        input.setPlayer(pad.id, 1)
        val start = key(pad.id, KeyEvent.KEYCODE_BUTTON_10, scan = 297)
        input.saveMapping(listOf(start to 8))
        input.key(start)
        assertEquals(0 to 8, input.buttons())
        input.onInputDeviceRemoved(pad.id)
        assertEquals(0 to 0, input.buttons())
        input.onInputDeviceAdded(pad.id)
        input.key(start)
        assertEquals(0 to 8, input.buttons())
        instrumentation.runOnMainSync { input.close(); input = ControllerInput(testContext) {} }
        assertEquals(1, input.controllers().first { it.id == pad.id }.player)
        input.key(start)
        assertEquals(0 to 8, input.buttons())
    }

    @Test fun changingPlayerReleasesHeldKeysAndAxesAndDisablingPersists() {
        val pad = connectedPad()
        input.setPlayer(pad.id, 0)
        val start = key(pad.id, KeyEvent.KEYCODE_BUTTON_START)
        input.key(start)
        val right = motion(pad.id, 1f)
        try { input.motion(right) } finally { right.recycle() }
        assertEquals(136 to 0, input.buttons())
        input.setPlayer(pad.id, 1)
        assertEquals(0 to 0, input.buttons())
        input.key(start)
        assertEquals(0 to 8, input.buttons())
        input.setPlayer(pad.id, null)
        assertEquals(0 to 0, input.buttons())
        assertFalse(input.key(start))
        instrumentation.runOnMainSync { input.close(); input = ControllerInput(testContext) {} }
        assertNull(input.controllers().first { it.id == pad.id }.player)
        assertFalse(input.key(start))
        assertEquals(0 to 0, input.buttons())
    }

    @Test fun savedSecondPlayerLeavesFirstPortForAnotherPadAndTouch() {
        val pad = connectedPad()
        input.setPlayer(pad.id, 1)
        instrumentation.runOnMainSync { input.close(); input = ControllerInput(testContext) {} }
        input.touch = 1
        input.key(key(10001, KeyEvent.KEYCODE_BUTTON_START))
        input.key(key(pad.id, KeyEvent.KEYCODE_BUTTON_A))
        assertEquals(9 to 2, input.buttons())
    }

    @Test fun keyboardCanBeExplicitlyAssignedToSecondPlayer() {
        val keyboard = InputDevice.getDeviceIds().map { InputDevice.getDevice(it) }.filterNotNull()
            .firstOrNull { !it.isVirtual && it.keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC }
        assumeNotNull(keyboard)
        input.setPlayer(keyboard!!.id, 1)
        input.key(key(keyboard.id, KeyEvent.KEYCODE_X, source = InputDevice.SOURCE_KEYBOARD))
        input.key(key(10001, KeyEvent.KEYCODE_BUTTON_START))
        assertEquals(8 to 1, input.buttons())
        assertEquals(1, input.controllers().first { it.id == keyboard.id }.player)
    }

    /** Covers the Pi's real adapter: it reports KEYBOARD | JOYSTICK, not GAMEPAD.
     * The K830 keyboard also claims JOYSTICK, but only for its volume axis. */
    @Test fun piKeyboardThenMappedUsbAdapterControlsPlayerOne() {
        val devices = InputDevice.getDeviceIds().map { InputDevice.getDevice(it) }.filterNotNull()
        val keyboard = devices.firstOrNull { it.name == "K830 Keyboard" }
        val gamepad = devices.firstOrNull { it.vendorId == 0x0810 && it.productId == 0xe501 }
        assumeNotNull(keyboard, gamepad)
        val keyboardId = keyboard!!.id
        val gamepadId = gamepad!!.id
        input.key(key(keyboardId, KeyEvent.KEYCODE_ENTER, source = InputDevice.SOURCE_KEYBOARD, scan = 28))
        input.key(key(keyboardId, KeyEvent.KEYCODE_ENTER, KeyEvent.ACTION_UP, InputDevice.SOURCE_KEYBOARD, 28))
        val start = key(gamepadId, KeyEvent.KEYCODE_BUTTON_10, source = InputDevice.SOURCE_KEYBOARD, scan = 297)
        input.saveMapping(listOf(
            key(gamepadId, KeyEvent.KEYCODE_BUTTON_2, source = InputDevice.SOURCE_KEYBOARD, scan = 289) to 1,
            key(gamepadId, KeyEvent.KEYCODE_BUTTON_1, source = InputDevice.SOURCE_KEYBOARD, scan = 288) to 2,
            key(gamepadId, KeyEvent.KEYCODE_BUTTON_9, source = InputDevice.SOURCE_KEYBOARD, scan = 296) to 4,
            start to 8,
        ))
        assertEquals(8, input.bitFor(start))
        input.key(key(gamepadId, KeyEvent.KEYCODE_BUTTON_10, KeyEvent.ACTION_UP, InputDevice.SOURCE_KEYBOARD, 297))
        repeat(3) {
            input.key(start)
            assertEquals(8 to 0, input.buttons())
            input.key(key(gamepadId, KeyEvent.KEYCODE_BUTTON_10, KeyEvent.ACTION_UP, InputDevice.SOURCE_KEYBOARD, 297))
            assertEquals(0 to 0, input.buttons())
        }
        val right = motion(gamepadId, 1f)
        try { input.motion(right) } finally { right.recycle() }
        assertEquals(128 to 0, input.buttons())
    }
}
