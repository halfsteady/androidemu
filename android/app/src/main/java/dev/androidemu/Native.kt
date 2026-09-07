package dev.androidemu

import java.nio.ByteBuffer

object Native {
    init { System.loadLibrary("nes_android") }
    external fun frameRate(): Float
    external fun load(rom: ByteArray): String
    external fun frame(buffer: ByteBuffer, p1: Int, p2: Int, advance: Boolean)
    external fun snapshot(battery: Boolean): ByteArray
    external fun restore(bytes: ByteArray, battery: Boolean)
    external fun audio(playing: Boolean)
}
