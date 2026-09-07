package dev.androidemu

/**
 * A look applied to the finished framebuffer. All of these are one fragment
 * shader with a branch, so switching costs nothing and there is one place to read.
 *
 * These are the cheap, fun end of §3 of the plan. The accurate end — the NTSC
 * composite filter, xBRZ and the CRT shader ports — needs real work and is not
 * pretending to be here.
 *
 * The ordinal is persisted, so append rather than reorder.
 */
enum class Filter(val label: String, val note: String) {
    None("Off", "The framebuffer exactly as the console drew it"),
    Scanlines("Scanlines", "A soft dark band between each row, the way a CRT drew them"),
    Crt("Old TV", "Scanlines, a curved tube, an aperture grille and a darkened edge"),
    DotMatrix("Dot matrix", "A grid between the pixels, like a handheld's LCD"),
    GameBoy("Four greens", "Everything remapped onto a handheld's four-shade screen"),
    Grey("Black and white", "The set nobody in the house wanted"),
    Sepia("Old photo", "Warm and faded"),
    Neon("Neon", "Bright things glow and the colour is turned all the way up");

    companion object {
        /** Persisted ordinals, tolerant of a value written by a newer build. */
        fun of(ordinal: Int) = entries.getOrElse(ordinal) { None }
    }
}
