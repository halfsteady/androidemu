package dev.androidemu

/**
 * Where a look's picture comes from before the present shader sees it.
 *
 * Most looks read the framebuffer straight. The ones that cannot are the ones
 * that need to work in picture space rather than screen space, where the same
 * work costs a hundredth as much: 61k or 246k fragments against the roughly 7.7
 * million a full-screen pass covers on this panel.
 */
enum class Source {
    /** The framebuffer as the console drew it. */
    Direct,

    /** Two scale2x steps, 4x, corners rounded off. */
    Smoothed,

    /** Encoded to a composite signal and decoded back, at four samples a pixel. */
    Composite,
}

/**
 * A look applied to the finished framebuffer.
 *
 * Most of these are one fragment shader with a branch, so switching costs
 * nothing and there is one place to read. A look that needs its picture prepared
 * first names a [Source] instead, and [ScreenRenderer] runs that off-screen
 * before presenting.
 *
 * [id] is what gets written to preferences, not the ordinal. A look that turns
 * out not to earn its place can then be removed without silently changing the
 * look everybody after it in the list is using. A retired id is never reused.
 */
enum class Filter(
    val id: Int,
    val label: String,
    val note: String,
    val source: Source = Source.Direct,
) {
    None(0, "Off", "The framebuffer exactly as the console drew it"),
    Scanlines(1, "Scanlines", "A soft dark band between each row, the way a CRT drew them"),
    Crt(2, "Old TV", "Scanlines, a curved tube, an aperture grille and a darkened edge"),
    DotMatrix(3, "Dot matrix", "A grid between the pixels, like a handheld's LCD"),
    GameBoy(4, "Four greens", "Everything remapped onto a handheld's four-shade screen"),
    // 5 was "Black and white". Cut: it is Sepia with the tint thrown away, and
    // anyone who had it now gets Sepia. The id stays retired.
    Sepia(6, "Old photo", "Warm and faded"),
    Neon(7, "Neon", "Bright things glow and the colour is turned all the way up"),
    Cartoon(8, "Cartoon", "Smoothed into curves, inked, and painted in flat colour", Source.Smoothed),
    // These two need no branch in the present shader: an unhandled kind already
    // draws the picture it was handed, and the interesting work already happened
    // to that picture off-screen.
    Smooth(9, "Smooth", "Stairsteps rounded into curves, with the colour left alone", Source.Smoothed),
    Composite(10, "Composite", "Down an aerial lead: colours bleed and dithering turns solid", Source.Composite);

    companion object {
        /** The id "Black and white" used to be saved under. */
        private const val RETIRED_GREY = 5

        /**
         * A saved id, tolerant of one written by a newer build. The first release
         * wrote ordinals, which for every surviving look are the same number.
         */
        fun of(id: Int) = when (id) {
            RETIRED_GREY -> Sepia
            else -> entries.firstOrNull { it.id == id } ?: None
        }
    }
}
