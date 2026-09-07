package dev.androidemu

/**
 * A framebuffer to preview filters against when no game is open. Built rather than
 * shipped as an image, and built to show what the filters actually do: flat colour
 * for the palette shifts, a smooth ramp for the ones that change contrast, a
 * one-pixel checkerboard for scanlines and the LCD grid, and a bright square for
 * the glow.
 */
object SampleFrame {
    /** 256×240 RGBA, top-down, the same layout the core writes. */
    fun pixels(): ByteArray {
        val out = ByteArray(Picture.WIDTH * Picture.HEIGHT * 4)
        fun put(x: Int, y: Int, r: Int, g: Int, b: Int) {
            val at = (y * Picture.WIDTH + x) * 4
            out[at] = r.toByte(); out[at + 1] = g.toByte(); out[at + 2] = b.toByte(); out[at + 3] = -1
        }
        for (y in 0 until Picture.HEIGHT) for (x in 0 until Picture.WIDTH) {
            val (r, g, b) = when {
                // Colour bars across the top, from the console's own palette.
                y < 72 -> BARS[(x * BARS.size / Picture.WIDTH).coerceAtMost(BARS.size - 1)]
                // A black-to-white ramp: what contrast and palette filters chew on.
                y < 120 -> Triple(x, x, x)
                // One-pixel checkerboard, which is what dithering looks like and
                // what scanlines and a dot-matrix grid have to sit on top of.
                y < 168 -> if ((x + y) % 2 == 0) Triple(232, 232, 240) else Triple(24, 24, 40)
                // A dark field with a few bright blocks, for the glow.
                else -> if (x % 64 in 16..47 && y % 40 in 8..31) Triple(248, 216, 96) else Triple(16, 20, 32)
            }
            put(x, y, r, g, b)
        }
        return out
    }

    // Eight of the 2C02's stronger colours, so a palette filter has something to
    // collapse and a saturation filter has something to push.
    private val BARS = listOf(
        Triple(236, 238, 236),
        Triple(248, 216, 120),
        Triple(88, 216, 84),
        Triple(0, 168, 68),
        Triple(88, 248, 252),
        Triple(60, 188, 252),
        Triple(216, 40, 0),
        Triple(148, 0, 132),
    )
}
