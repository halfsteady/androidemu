package dev.androidemu

import org.junit.Assert.*
import org.junit.Test
import org.junit.After
import java.nio.ByteBuffer

/** Exercises the real Kotlin/JNI bridge against the host build of the Rust core. */
class NativeBridgeTest {
    @After fun restoreRewindDefault() { Native.setRewindEnabled(true) }
    private fun rom(): ByteArray = ByteArray(16 + 16384).apply {
        byteArrayOf(0x4e, 0x45, 0x53, 0x1a).copyInto(this)
        this[4] = 1; this[6] = 2
        byteArrayOf(0x4c, 0, 0x80.toByte()).copyInto(this, 16)
        this[16 + 0x3ffd] = 0x80.toByte()
    }
    @Test fun videoStateAndBatteryRoundTrip() {
        val id = Native.load(rom()); assertEquals(16, id.length)
        val buffer = ByteBuffer.allocateDirect(256 * 240 * 4)
        Native.frame(buffer, 0, 0, true)
        assertEquals(255, buffer.get(3).toInt() and 255)
        val saved = Native.snapshot(false)
        Native.frame(buffer, 0x91, 0, true)
        val expected = Native.snapshot(false)
        Native.restore(saved, false)
        Native.frame(buffer, 0x91, 0, true)
        assertArrayEquals(expected, Native.snapshot(false))
        val battery = Native.snapshot(true); assertEquals(8192, battery.size)
        battery[100] = 42; Native.restore(battery, true)
        assertArrayEquals(battery, Native.snapshot(true))
    }
    @Test fun rewindWalksBackThroughTheFramesJustPlayed() {
        Native.load(rom())
        val buffer = ByteBuffer.allocateDirect(256 * 240 * 4)
        // Frame 0 is recorded before it runs, so after five frames the chain
        // holds the five states that preceded the current one.
        val states = ArrayList<ByteArray>()
        repeat(5) { states.add(Native.snapshot(false)); Native.frame(buffer, 0, 0, true) }
        assertEquals(5, Native.rewindDepth())
        for (expected in states.reversed()) {
            assertTrue("the chain ran out early", Native.rewind(buffer))
            assertArrayEquals(expected, Native.snapshot(false))
        }
        // Past the oldest frame it reports that it stopped, and leaves the
        // machine where it was rather than unwinding into nothing.
        val oldest = Native.snapshot(false)
        assertFalse(Native.rewind(buffer))
        assertArrayEquals(oldest, Native.snapshot(false))
        assertEquals(0, Native.rewindDepth())
    }
    @Test fun loadingAStateAbandonsTheRewindChain() {
        Native.load(rom())
        val buffer = ByteBuffer.allocateDirect(256 * 240 * 4)
        repeat(4) { Native.frame(buffer, 0, 0, true) }
        val saved = Native.snapshot(false)
        assertTrue(Native.rewindDepth() > 0)
        // The chain led back from a different moment, so it does not survive.
        Native.restore(saved, false)
        assertEquals(0, Native.rewindDepth())
        assertFalse(Native.rewind(buffer))
    }
    @Test fun disablingRewindKeepsEmulationAndManualSavesIdentical() {
        val buffer = ByteBuffer.allocateDirect(256 * 240 * 4)
        Native.load(rom())
        val initial = Native.snapshot(false)
        repeat(20) { Native.frame(buffer, it * 7, it * 3, true) }
        val expected = Native.snapshot(false)
        val battery = Native.snapshot(true)
        assertEquals(20, Native.rewindDepth())

        Native.setRewindEnabled(false)
        assertEquals(0, Native.rewindDepth())
        assertArrayEquals(expected, Native.snapshot(false))
        Native.restore(initial, false)
        repeat(20) { Native.frame(buffer, it * 7, it * 3, true) }
        assertArrayEquals(expected, Native.snapshot(false))
        assertArrayEquals(battery, Native.snapshot(true))
        assertEquals(0, Native.rewindDepth())
        assertFalse(Native.rewind(buffer))

        Native.setRewindEnabled(true)
        assertEquals(0, Native.rewindDepth())
        Native.frame(buffer, 0, 0, true)
        assertEquals(1, Native.rewindDepth())
        assertTrue(Native.rewind(buffer))
        assertArrayEquals(expected, Native.snapshot(false))
    }

    @Test fun disabledRewindStaysDisabledAcrossLoadResetAndRestore() {
        Native.setRewindEnabled(false)
        val bytes = rom()
        Native.load(bytes)
        val buffer = ByteBuffer.allocateDirect(256 * 240 * 4)
        val initial = Native.snapshot(false)
        repeat(3) { Native.frame(buffer, 0, 0, true) }
        assertEquals(0, Native.rewindDepth())
        Native.reset(bytes)
        Native.frame(buffer, 0, 0, true)
        assertEquals(0, Native.rewindDepth())
        Native.restore(initial, false)
        Native.frame(buffer, 0, 0, true)
        assertEquals(0, Native.rewindDepth())
        Native.setRewindEnabled(true)
        Native.frame(buffer, 0, 0, true)
        assertEquals(1, Native.rewindDepth())
        Native.setRewindEnabled(true) // Reapplying the preference must retain the chain.
        assertEquals(1, Native.rewindDepth())
    }

    @Test fun errorsBecomeExceptionsAndPreserveTheSession() {
        Native.load(rom()); val saved = Native.snapshot(false)
        fun rejects(block: () -> Unit) {
            try { block(); fail("Expected a native error") } catch (_: IllegalStateException) {}
            assertArrayEquals(saved, Native.snapshot(false))
        }
        rejects { Native.load(byteArrayOf(1,2,3)) }
        rejects { Native.frame(ByteBuffer.allocateDirect(8), 0,0,true) }
        rejects { Native.frame(ByteBuffer.allocate(256 * 240 * 4),0,0,true) }
        rejects { Native.restore(saved.copyOf(20), false) }
        rejects { Native.restore(byteArrayOf(1,2,3), true) }
        val corrupt = saved.clone(); corrupt[100] = (corrupt[100].toInt() xor 1).toByte()
        rejects { Native.restore(corrupt, false) }
    }
    @Test fun resetPowersOnFreshKeepsBatteryAndClearsRewind() {
        val bytes = rom()
        Native.load(bytes)
        val battery = Native.snapshot(true).apply { this[0] = 7; this[100] = 42; this[lastIndex] = 9 }
        Native.restore(battery, true)
        val fresh = Native.snapshot(false)
        val buffer = ByteBuffer.allocateDirect(256 * 240 * 4)
        repeat(6) { Native.frame(buffer, 0x91, 0x28, true) }
        val manual = Native.snapshot(false)
        assertFalse(fresh.contentEquals(manual))
        assertTrue(Native.rewindDepth() > 0)

        Native.reset(bytes)
        assertArrayEquals(fresh, Native.snapshot(false))
        assertArrayEquals(battery, Native.snapshot(true))
        assertEquals(0, Native.rewindDepth())
        assertFalse(Native.rewind(buffer))
        // Reset does not invalidate a manual save made earlier in the game.
        Native.restore(manual, false)
        assertArrayEquals(manual, Native.snapshot(false))
    }
    @Test fun resetWithoutBatteryRejectsWrongRomsWithoutChangingTheSession() {
        val bytes = rom().apply { this[6] = 0 }
        Native.load(bytes)
        val fresh = Native.snapshot(false)
        val buffer = ByteBuffer.allocateDirect(256 * 240 * 4)
        repeat(3) { Native.frame(buffer, 0, 0, true) }
        val saved = Native.snapshot(false)
        val depth = Native.rewindDepth()
        for (bad in listOf(byteArrayOf(1, 2, 3), bytes.clone().apply { this[24] = 7 }, bytes.clone().apply { this[6] = 2 })) {
            try { Native.reset(bad); fail("Wrong ROM was accepted for reset") } catch (_: IllegalStateException) {}
            assertArrayEquals(saved, Native.snapshot(false))
            assertEquals(depth, Native.rewindDepth())
        }
        Native.reset(bytes)
        assertArrayEquals(fresh, Native.snapshot(false))
        assertEquals(0, Native.snapshot(true).size)
        assertEquals(0, Native.rewindDepth())
    }
}
