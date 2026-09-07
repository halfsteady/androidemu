package dev.androidemu

import android.content.Context
import android.hardware.input.InputManager
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import java.util.concurrent.ConcurrentHashMap

class ControllerInput(context: Context, private val disconnected: () -> Unit) : InputManager.InputDeviceListener {
    private val preferences = context.getSharedPreferences("controllers", Context.MODE_PRIVATE)
    private fun profile(event: KeyEvent) = event.device?.let { "${it.vendorId}:${it.productId}:${it.descriptor}" } ?: "keyboard"
    private fun physical(event: KeyEvent) = if (event.scanCode != 0) "scan:${event.scanCode}" else "key:${event.keyCode}"
    fun saveMapping(events: List<Pair<KeyEvent, Int>>) {
        val edit = preferences.edit()
        events.groupBy { profile(it.first) }.forEach { (device, mappings) ->
            preferences.all.keys.filter { it.startsWith("$device/") }.forEach { edit.remove(it) }
            mappings.forEach { (event, bit) -> edit.putInt("$device/${physical(event)}", bit) }
        }
        edit.apply(); clear()
    }
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
    // The NES button bit this event stands for, or 0 for a key we don't use.
    // A saved profile wins over the built-in guesses, which is the whole point
    // of the mapping wizard: cheap adapters report buttons the defaults miss.
    fun bitFor(event: KeyEvent): Int =
        preferences.getInt("${profile(event)}/${physical(event)}", 0).takeIf { it != 0 } ?: when (event.keyCode) {
            // NES A is the right-hand button, B the left. Android calls the
            // bottom face button BUTTON_A, and that is where a thumb rests, so
            // it drives NES B while BUTTON_B drives NES A. Getting this the
            // other way round makes every game feel backwards.
            KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_BUTTON_2, KeyEvent.KEYCODE_X -> 1
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_1, KeyEvent.KEYCODE_Z -> 2
            KeyEvent.KEYCODE_BUTTON_SELECT, KeyEvent.KEYCODE_SHIFT_RIGHT -> 4
            KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_ENTER -> 8
            KeyEvent.KEYCODE_DPAD_UP -> 16
            KeyEvent.KEYCODE_DPAD_DOWN -> 32
            KeyEvent.KEYCODE_DPAD_LEFT -> 64
            KeyEvent.KEYCODE_DPAD_RIGHT -> 128
            else -> 0
        }
    fun key(event: KeyEvent): Boolean {
        val bit = bitFor(event)
        if (bit == 0 || !assign(event.deviceId)) return false
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
