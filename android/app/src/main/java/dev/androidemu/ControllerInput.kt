package dev.androidemu

import android.content.Context
import android.hardware.input.InputManager
import android.view.InputDevice
import android.view.InputEvent
import android.view.KeyEvent
import android.view.MotionEvent
import java.util.concurrent.ConcurrentHashMap

data class ConnectedController(
    val id: Int,
    val name: String,
    val gamepad: Boolean,
    val player: Int?,
    val customMapping: Boolean,
) {
    val playerLabel: String get() = player?.let { "Player ${it + 1}" } ?: "Not assigned"
}

class ControllerInput(context: Context, private val disconnected: () -> Unit) : InputManager.InputDeviceListener {
    private val preferences = context.getSharedPreferences("controllers", Context.MODE_PRIVATE)
    private fun profile(device: InputDevice) = "${device.vendorId}:${device.productId}:${device.descriptor}"
    private fun profile(event: InputEvent) = event.device?.let(::profile) ?: "keyboard"
    var devicesChanged: () -> Unit = {}
    private fun physical(event: KeyEvent) = if (event.scanCode != 0) "scan:${event.scanCode}" else "key:${event.keyCode}"
    fun saveMapping(events: List<Pair<KeyEvent, Int>>) {
        val edit = preferences.edit()
        events.groupBy { profile(it.first) }.forEach { (device, mappings) ->
            preferences.all.keys.filter { it.startsWith("$device/") }.forEach { edit.remove(it) }
            mappings.forEach { (event, bit) -> edit.putInt("$device/${physical(event)}", bit) }
        }
        edit.apply(); clear(); devicesChanged()
    }
    private val manager = context.getSystemService(InputManager::class.java)
    // Keyboards share P1 by default; only gamepads reserve automatic ports.
    // Explicit assignments apply to either kind and survive Android device-ID changes.
    private val slots = ConcurrentHashMap<Int, Int>()
    private val gamepads = mutableSetOf<Int>()
    private val keys = ConcurrentHashMap<Int, Int>()
    private val axes = ConcurrentHashMap<Int, Int>()
    @Volatile var touch = 0
    init { manager.registerInputDeviceListener(this, null) }
    fun close() { manager.unregisterInputDeviceListener(this) }
    fun clear() { keys.clear(); axes.clear(); touch = 0 }
    fun buttons(): Pair<Int, Int> {
        var p1 = touch; var p2 = 0
        keys.forEach { (id, bits) -> if (slots[id] == 1) p2 = p2 or bits else p1 = p1 or bits }
        axes.forEach { (id, bits) -> if (slots[id] == 1) p2 = p2 or bits else p1 = p1 or bits }
        return p1 to p2
    }
    private fun isGamepad(device: InputDevice): Boolean =
        // Cheap USB adapters can report KEYBOARD | JOYSTICK without GAMEPAD.
        // K830 advertises JOYSTICK for volume alone, so require directional axes.
        device.supportsSource(InputDevice.SOURCE_GAMEPAD) ||
            listOf(MotionEvent.AXIS_X, MotionEvent.AXIS_Y, MotionEvent.AXIS_HAT_X, MotionEvent.AXIS_HAT_Y)
                .any { device.getMotionRange(it, InputDevice.SOURCE_JOYSTICK) != null }

    private fun isGamepad(event: InputEvent): Boolean = event.device?.let(::isGamepad) ?: (
        event.isFromSource(InputDevice.SOURCE_GAMEPAD) || event.isFromSource(InputDevice.SOURCE_JOYSTICK))

    private fun availableDevices(): List<InputDevice> = manager.inputDeviceIds.map(manager::getInputDevice).filterNotNull()
        .filter { !it.isVirtual && (isGamepad(it) || it.keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC ||
            it.supportsSource(InputDevice.SOURCE_DPAD)) }.sortedBy { it.id }

    private fun assign(id: Int, profileKey: String, gamepad: Boolean): Boolean {
        if (slots.containsKey(id)) return true
        if (gamepad) gamepads.add(id)
        // Separate namespace: remapping buttons must not erase the player choice.
        val preferred = preferences.getInt("port/$profileKey", -2)
        if (preferred == -1) return false
        val slot = if (preferred in 0..1) preferred else if (!gamepad) 0 else {
            // Honour saved assignments even if the other pad hasn't sent a key yet.
            val reserved = availableDevices().filter(::isGamepad)
                .map { preferences.getInt("port/${profile(it)}", -2) }
            (0..1).firstOrNull { port -> gamepads.none { slots[it] == port } && port !in reserved } ?: return false
        }
        slots[id] = slot
        return true
    }

    fun accepts(event: InputEvent): Boolean = assign(event.deviceId, profile(event), isGamepad(event))

    /** The list shows the same routes that gameplay uses, including default ports. */
    fun controllers(): List<ConnectedController> {
        val devices = availableDevices()
        // Resolve explicit choices before allocating automatic ports.
        devices.sortedBy { if (preferences.contains("port/${profile(it)}")) 0 else 1 }.forEach {
            assign(it.id, profile(it), isGamepad(it))
        }
        return devices.map {
            ConnectedController(it.id, it.name.trim(), isGamepad(it), slots[it.id],
                preferences.all.keys.any { key -> key.startsWith("${profile(it)}/") })
        }
    }

    /** null disables gameplay for this device; menu navigation still works. */
    fun setPlayer(id: Int, player: Int?) {
        require(player == null || player in 0..1)
        val device = manager.getInputDevice(id) ?: return
        preferences.edit().putInt("port/${profile(device)}", player ?: -1).apply()
        keys.remove(id); axes.remove(id); slots.remove(id)
        if (player != null) slots[id] = player
        if (isGamepad(device)) gamepads.add(id)
        devicesChanged()
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
        if (bit == 0) return false
        // The wizard or pause menu can consume DOWN before gameplay receives
        // UP. A release alone must not reserve a port or create held state.
        if (event.action == KeyEvent.ACTION_UP) {
            keys[event.deviceId]?.let { keys[event.deviceId] = it and bit.inv() }
            return true
        }
        if (event.action != KeyEvent.ACTION_DOWN || !accepts(event)) return false
        val old = keys[event.deviceId] ?: 0
        keys[event.deviceId] = old or bit
        return true
    }
    fun motion(event: MotionEvent): Boolean {
        if (!event.isFromSource(InputDevice.SOURCE_JOYSTICK) ||
            event.action != MotionEvent.ACTION_MOVE || !isGamepad(event)) return false
        val x = event.getAxisValue(MotionEvent.AXIS_HAT_X).takeIf { it != 0f } ?: event.getAxisValue(MotionEvent.AXIS_X)
        val y = event.getAxisValue(MotionEvent.AXIS_HAT_Y).takeIf { it != 0f } ?: event.getAxisValue(MotionEvent.AXIS_Y)
        val bits = (if (x < -0.5f) 64 else if (x > 0.5f) 128 else 0) or (if (y < -0.5f) 16 else if (y > 0.5f) 32 else 0)
        if (bits == 0 && !axes.containsKey(event.deviceId)) return false
        if (!accepts(event)) return false
        axes[event.deviceId] = bits
        return true
    }
    override fun onInputDeviceAdded(id: Int) { devicesChanged() }
    override fun onInputDeviceChanged(id: Int) { keys.remove(id); axes.remove(id); devicesChanged() }
    override fun onInputDeviceRemoved(id: Int) {
        val hadKeys = keys.remove(id) != null
        val hadAxes = axes.remove(id) != null
        val hadSlot = slots.remove(id) != null
        gamepads.remove(id)
        devicesChanged()
        if (hadKeys || hadAxes || hadSlot) disconnected()
    }
}
