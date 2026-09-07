package dev.androidemu

import java.nio.ByteBuffer

object Native {
    init { System.loadLibrary("nes_android") }
    external fun frameRate(): Float
    external fun load(rom: ByteArray): String
    external fun frame(buffer: ByteBuffer, p1: Int, p2: Int, advance: Boolean)
    /** Steps one frame back and paints it. False once there is nothing left to undo. */
    external fun rewind(buffer: ByteBuffer): Boolean
    external fun rewindDepth(): Int
    external fun snapshot(battery: Boolean): ByteArray
    external fun restore(bytes: ByteArray, battery: Boolean)
    external fun audio(playing: Boolean)
}
