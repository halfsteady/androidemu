package dev.androidemu

import org.junit.Assert.*
import org.junit.Test

/**
 * The presentation geometry, checked without a device. Getting this wrong shows up
 * as a stretched picture or a one-pixel shimmer, both of which are easy to ship and
 * hard to notice in a screenshot.
 */
class PictureTest {
    private fun aspectOf(view: Viewport) = view.width.toFloat() / view.height

    @Test fun televisionIsFourThirdsAndFitsTheSurface() {
        val wide = Picture.layout(1000, 500, Aspect.Television, false)
        assertEquals(4f / 3f, aspectOf(wide), 0.01f)
        assertTrue("$wide overflows", wide.width <= 1000 && wide.height <= 500)
        // A tall surface is limited by width instead, and still lands on 4:3.
        val tall = Picture.layout(600, 2000, Aspect.Television, false)
        assertEquals(4f / 3f, aspectOf(tall), 0.01f)
        assertEquals(600, tall.width)
    }

    @Test fun hardwareIsNarrowerThanTelevision() {
        val television = Picture.layout(1000, 1000, Aspect.Television, false)
        val hardware = Picture.layout(1000, 1000, Aspect.Hardware, false)
        // 8:7 pixels over 240 rows come out at about 1.22, against 1.33.
        assertEquals(256f * 8f / 7f / 240f, aspectOf(hardware), 0.01f)
        assertTrue("8:7 should be taller than 4:3 in the same box", hardware.height > television.height)
    }

    @Test fun trimmingRaisesTheAspectAndNarrowsTheSource() {
        assertEquals(240, Picture.visibleHeight(false))
        assertEquals(224, Picture.visibleHeight(true))
        assertEquals(0f, Picture.trimFraction(false), 0f)
        assertEquals(8f / 240f, Picture.trimFraction(true), 0.0001f)
        // Fewer visible rows at the same pixel aspect means a wider picture.
        val whole = Picture.layout(1000, 1000, Aspect.Hardware, false)
        val trimmed = Picture.layout(1000, 1000, Aspect.Hardware, true)
        assertTrue(aspectOf(trimmed) > aspectOf(whole))
    }

    @Test fun pixelPerfectUsesWholeMultiplesOnly() {
        // An exact fit takes the whole surface.
        val exact = Picture.layout(1024, 960, Aspect.Pixels, false)
        assertEquals(Viewport(0, 0, 1024, 960), exact)
        // An awkward surface drops to the next whole scale rather than resampling.
        val awkward = Picture.layout(1000, 900, Aspect.Pixels, false)
        assertEquals(Viewport(116, 90, 768, 720), awkward)
        // Trimmed, 224 rows fit a fourth multiple where 240 rows would not.
        assertEquals(Viewport(0, 2, 1024, 896), Picture.layout(1024, 900, Aspect.Pixels, true))
    }

    @Test fun everyLayoutIsCentredAndInsideTheSurface() {
        for (aspect in Aspect.entries) for (trim in listOf(false, true)) {
            for (width in listOf(1, 320, 721, 1000, 2400)) for (height in listOf(1, 240, 519, 900, 1600)) {
                val view = Picture.layout(width, height, aspect, trim)
                assertEquals("$aspect $trim ${width}x$height not centred", (width - view.width) / 2, view.x)
                assertEquals("$aspect $trim ${width}x$height not centred", (height - view.height) / 2, view.y)
                if (aspect != Aspect.Pixels) {
                    // Pixel-perfect keeps at least a 1× picture even on a surface
                    // too small for it; the scaling modes must always fit.
                    assertTrue("$aspect $trim ${width}x$height overflows", view.width <= width && view.height <= height)
                }
            }
        }
    }

    @Test fun aDegenerateSurfaceDrawsNothing() {
        assertEquals(Viewport(0, 0, 0, 0), Picture.layout(0, 0, Aspect.Television, false))
        assertEquals(Viewport(0, 0, 0, 0), Picture.layout(-4, 100, Aspect.Pixels, false))
    }
}
