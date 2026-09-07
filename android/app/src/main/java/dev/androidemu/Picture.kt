package dev.androidemu

/** How the framebuffer is shaped on screen. */
enum class Aspect(val label: String) {
    /** 4:3, the way a television showed it. The default, because that is what these games were drawn for. */
    Television("4:3 television"),

    /** The 2C02's own 8:7 pixel aspect — narrower, and what the hardware actually emitted. */
    Hardware("8:7 hardware"),

    /** Square pixels at a whole-number scale. Every NES pixel becomes the same block of screen pixels. */
    Pixels("Pixel-perfect"),
}

/** Where the framebuffer lands, in pixels, with the origin at the bottom left as GL wants it. */
data class Viewport(val x: Int, val y: Int, val width: Int, val height: Int)

/**
 * The geometry of presenting a 256×240 framebuffer, kept apart from the GL code
 * so it can be reasoned about and tested without a device.
 */
object Picture {
    const val WIDTH = 256
    const val HEIGHT = 240

    /**
     * Rows hidden at the top and bottom when edges are trimmed. A real television
     * lost roughly this much to overscan, which is why so many games leave status
     * bar seams, partial tiles and scroll garbage up there.
     */
    const val TRIM = 8

    /** The 2C02 emitted pixels 8/7 as wide as they were tall. */
    private const val PIXEL_ASPECT = 8f / 7f

    fun visibleHeight(trim: Boolean) = if (trim) HEIGHT - 2 * TRIM else HEIGHT

    /** The fraction of the framebuffer's height cut from each edge, for texture coordinates. */
    fun trimFraction(trim: Boolean) = if (trim) TRIM.toFloat() / HEIGHT else 0f

    /**
     * The largest presentation of the visible area that fits the surface, centred.
     * Television and hardware modes scale continuously; pixel-perfect drops to the
     * next whole multiple, so it will leave a wider border rather than resample.
     */
    fun layout(surfaceWidth: Int, surfaceHeight: Int, aspect: Aspect, trim: Boolean): Viewport {
        val visible = visibleHeight(trim)
        if (surfaceWidth <= 0 || surfaceHeight <= 0) return Viewport(0, 0, 0, 0)
        val (width, height) = when (aspect) {
            // A whole-number scale in both directions, so no NES pixel is ever
            // wider than its neighbour. At least 1, or a tiny surface shows nothing.
            Aspect.Pixels -> {
                val scale = minOf(surfaceWidth / WIDTH, surfaceHeight / visible).coerceAtLeast(1)
                WIDTH * scale to visible * scale
            }
            else -> {
                val target = if (aspect == Aspect.Television) 4f / 3f else WIDTH * PIXEL_ASPECT / visible
                // Fit the taller-or-wider constraint, whichever binds first.
                val width = minOf(surfaceWidth.toFloat(), surfaceHeight * target)
                width.toInt() to (width / target).toInt()
            }
        }
        return Viewport((surfaceWidth - width) / 2, (surfaceHeight - height) / 2, width, height)
    }
}
