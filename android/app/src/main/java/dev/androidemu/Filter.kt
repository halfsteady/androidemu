package dev.androidemu

/**
 * A look applied to the finished framebuffer.
 *
 * Most of these are one fragment shader with a branch, so switching costs nothing
 * and there is one place to read. [Cartoon] is the exception: it needs the picture
 * smoothed before it can be inked, so it asks for the source passes in
 * [ScreenRenderer] first.
 *
 * [id] is what gets written to preferences, not the ordinal. A look that turns out
 * not to earn its place can then be removed without silently changing the look
 * everybody after it in the list is using. A retired id is never reused.
 */
enum class Filter(
    val id: Int,
    val label: String,
    val note: String,
    /** Whether the picture is edge-smoothed into an off-screen buffer first. */
    val smooths: Boolean = false,
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
    Cartoon(8, "Cartoon", "Smoothed into curves, inked, and painted in flat colour", smooths = true);

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
