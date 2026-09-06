package dev.androidemu

import android.content.Context
import android.hardware.input.InputManager
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import java.util.concurrent.ConcurrentHashMap

class ControllerInput(context: Context, private val disconnected: () -> Unit) : InputManager.InputDeviceListener {
    private val manager = context.getSystemService(InputManager::class.java)
    private val slots = ConcurrentHashMap<Int, Int>()
    private val keys = ConcurrentHashMap<Int, Int>()
    private val axes = ConcurrentHashMap<Int, Int>()
    @Volatile var touch = 0
    init { manager.registerInputDeviceListener(this, null) }
    fun close() { manager.unregisterInputDeviceListener(this) }
    fun clear() { keys.clear(); axes.clear(); touch = 0 }
    fun buttons(): Pair<Int, Int> {
        var p1 = touch; var p2 = 0
        slots.forEach { (id, slot) -> val bits = (keys[id] ?: 0) or (axes[id] ?: 0); if (slot == 0) p1 = p1 or bits else p2 = p2 or bits }
        return p1 to p2
    }
    private fun assign(id: Int): Boolean {
        if (slots.containsKey(id)) return true
        val slot = (0..1).firstOrNull { !slots.containsValue(it) } ?: return false
        slots[id] = slot; return true
    }
    fun key(event: KeyEvent): Boolean {
        val bit = when (event.keyCode) {
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_X -> 1
            KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_Z -> 2
            KeyEvent.KEYCODE_BUTTON_SELECT, KeyEvent.KEYCODE_SHIFT_RIGHT -> 4
            KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_ENTER -> 8
            KeyEvent.KEYCODE_DPAD_UP -> 16; KeyEvent.KEYCODE_DPAD_DOWN -> 32
            KeyEvent.KEYCODE_DPAD_LEFT -> 64; KeyEvent.KEYCODE_DPAD_RIGHT -> 128
            else -> return false
        }
        if (!assign(event.deviceId)) return false
        val old = keys[event.deviceId] ?: 0
        keys[event.deviceId] = if (event.action == KeyEvent.ACTION_DOWN) old or bit else old and bit.inv()
        return true
    }
    fun motion(event: MotionEvent): Boolean {
        if (event.source and InputDevice.SOURCE_JOYSTICK != InputDevice.SOURCE_JOYSTICK || !assign(event.deviceId)) return false
        val x = event.getAxisValue(MotionEvent.AXIS_HAT_X).takeIf { it != 0f } ?: event.getAxisValue(MotionEvent.AXIS_X)
        val y = event.getAxisValue(MotionEvent.AXIS_HAT_Y).takeIf { it != 0f } ?: event.getAxisValue(MotionEvent.AXIS_Y)
        axes[event.deviceId] = (if (x < -0.5f) 64 else if (x > 0.5f) 128 else 0) or (if (y < -0.5f) 16 else if (y > 0.5f) 32 else 0)
        return true
    }
    override fun onInputDeviceAdded(id: Int) {}
    override fun onInputDeviceChanged(id: Int) { keys.remove(id); axes.remove(id) }
    override fun onInputDeviceRemoved(id: Int) { val used = slots.remove(id) != null; keys.remove(id); axes.remove(id); if (used) disconnected() }
}
