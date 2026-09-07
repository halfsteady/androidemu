package dev.androidemu

import java.nio.ByteBuffer

object Native {
    init { System.loadLibrary("nes_android") }
    external fun frameRate(): Float
    external fun load(rom: ByteArray): String
    /**
     * Plain sentences for whatever the loaded ROM's header had to have corrected.
     * Empty when the header was believed as written.
     */
    external fun headerNotes(): Array<String>
    external fun frame(buffer: ByteBuffer, p1: Int, p2: Int, advance: Boolean)
    /** Steps one frame back and paints it. False once there is nothing left to undo. */
    external fun rewind(buffer: ByteBuffer): Boolean
    external fun rewindDepth(): Int
    external fun snapshot(battery: Boolean): ByteArray
    external fun restore(bytes: ByteArray, battery: Boolean)
    external fun audio(playing: Boolean)
    /**
     * Where the audio latency is, in milliseconds: the queue, the device ring,
     * the two added, the target the controller settled on, and then the underrun
     * count. A number rather than a claim — see docs/AUDIO.md.
     */
    external fun audioStats(): FloatArray
}
