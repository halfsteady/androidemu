package dev.androidemu

/**
 * A framebuffer to preview filters against when no game is open. Built rather than
 * shipped as an image, and built to show what the filters actually do: flat colour
 * for the palette shifts, a smooth ramp for the ones that change contrast, a
 * one-pixel checkerboard for scanlines and the LCD grid, and a bright square for
 * the glow.
 */
object SampleFrame {
    /**
     * 256×240 RGBA, top-down, the same layout the core writes.
     *
     * [palette] is the 64 colours currently in force, so choosing a palette moves
     * the bars the way it moves a game. Defaults to the model's standard set for
     * callers that only want something for the filters to chew on.
     */
    fun pixels(palette: IntArray = PaletteModel.standard()): ByteArray {
        val bars = BAR_INDICES.map { at ->
            val c = palette[at]
            Triple((c shr 16) and 255, (c shr 8) and 255, c and 255)
        }
        val out = ByteArray(Picture.WIDTH * Picture.HEIGHT * 4)
        fun put(x: Int, y: Int, r: Int, g: Int, b: Int) {
            val at = (y * Picture.WIDTH + x) * 4
            out[at] = r.toByte(); out[at + 1] = g.toByte(); out[at + 2] = b.toByte(); out[at + 3] = -1
        }
        for (y in 0 until Picture.HEIGHT) for (x in 0 until Picture.WIDTH) {
            val (r, g, b) = when {
                // Colour bars across the top, from the console's own palette.
                y < 72 -> bars[(x * bars.size / Picture.WIDTH).coerceAtMost(bars.size - 1)]
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

    // Eight of the 2C02's stronger colours, by index rather than by value, so a
    // palette change restains the bars instead of leaving them behind.
    private val BAR_INDICES = listOf(0x30, 0x28, 0x2a, 0x1a, 0x2c, 0x21, 0x16, 0x14)
}
