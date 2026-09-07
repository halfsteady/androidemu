package dev.androidemu

import android.opengl.GLES30.*
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * The one piece of GL in the app: a textured quad and the filter shader. The game
 * view draws through it every frame, and the settings preview draws through it into
 * an off-screen buffer, so what the preview shows is what the game will look like
 * rather than a second implementation that drifts.
 */
class ScreenRenderer {
    private var program = 0
    private var texture = 0
    // Looked up once. draw() is the per-frame path and a string lookup per
    // uniform per frame is pure waste.
    private var uScreen = 0
    private var uKind = 0
    private var uCols = 0
    private var uRows = 0
    private var uWindow = 0
    private var frame = 0
    private var frameTexture = 0
    private var frameWidth = 0
    private var frameHeight = 0
    private val vertices = ByteBuffer.allocateDirect(16 * 4).order(ByteOrder.nativeOrder()).asFloatBuffer()
    private var readback: ByteBuffer? = null

    // The source-pass chain. Two doublings of the framebuffer, each one a
    // scale2x that rounds off the corners a NES pixel leaves behind. Both live
    // in picture space rather than screen space, so they cost the same whatever
    // the display is doing: 256x240 and 512x480 fragments against the roughly
    // 7.7 million a full-screen pass would have to cover.
    private var smoothProgram = 0
    private var uSmoothSrc = 0
    private var uSmoothSize = 0
    private val smoothBuffers = IntArray(SMOOTH_STEPS)
    private val smoothTextures = IntArray(SMOOTH_STEPS)
    private val viewport = IntArray(4)
    private val boundFrame = IntArray(1)

    val ready get() = program != 0

    fun create() {
        program = link(FRAGMENT)
        uScreen = glGetUniformLocation(program, "screen")
        uKind = glGetUniformLocation(program, "kind")
        uCols = glGetUniformLocation(program, "cols")
        uRows = glGetUniformLocation(program, "rows")
        uWindow = glGetUniformLocation(program, "window")
        smoothProgram = link(SMOOTH_FRAGMENT)
        uSmoothSrc = glGetUniformLocation(smoothProgram, "src")
        uSmoothSize = glGetUniformLocation(smoothProgram, "srcSize")
        val textures = IntArray(1); glGenTextures(1, textures, 0); texture = textures[0]
        glBindTexture(GL_TEXTURE_2D, texture)
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST)
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST)
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE)
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE)
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, Picture.WIDTH, Picture.HEIGHT, 0, GL_RGBA, GL_UNSIGNED_BYTE, null)
        createSmoothChain()
    }

    /**
     * The two off-screen steps of the smoothing chain. The last one is the only
     * texture the picture is ever magnified from, so it carries mipmaps: the
     * glow reads one of the small levels instead of spending taps on a blur.
     */
    private fun createSmoothChain() {
        glGenFramebuffers(SMOOTH_STEPS, smoothBuffers, 0)
        glGenTextures(SMOOTH_STEPS, smoothTextures, 0)
        for (step in 0 until SMOOTH_STEPS) {
            val scale = 1 shl (step + 1)
            val last = step == SMOOTH_STEPS - 1
            glBindTexture(GL_TEXTURE_2D, smoothTextures[step])
            // Every step but the last is read by scale2x, which compares texels
            // for equality and so must never see a filtered sample.
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, if (last) GL_LINEAR_MIPMAP_LINEAR else GL_NEAREST)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, if (last) GL_LINEAR else GL_NEAREST)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE)
            glTexImage2D(
                GL_TEXTURE_2D, 0, GL_RGBA, Picture.WIDTH * scale, Picture.HEIGHT * scale,
                0, GL_RGBA, GL_UNSIGNED_BYTE, null
            )
            if (last) glGenerateMipmap(GL_TEXTURE_2D)
            glBindFramebuffer(GL_FRAMEBUFFER, smoothBuffers[step])
            glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, smoothTextures[step], 0)
            val status = glCheckFramebufferStatus(GL_FRAMEBUFFER)
            check(status == GL_FRAMEBUFFER_COMPLETE) { "smoothing buffer $step incomplete: $status" }
        }
        glBindFramebuffer(GL_FRAMEBUFFER, 0)
    }

    /**
     * Points the viewport and the quad at the visible part of the framebuffer. The
     * framebuffer is always uploaded whole; trimming moves the window the quad
     * samples, which keeps the upload one unconditional call. Texture rows run
     * top-down while the quad runs bottom-up, so the top of the picture is
     * v = trim and the bottom is 1 - trim.
     */
    fun place(surfaceWidth: Int, surfaceHeight: Int, aspect: Aspect, trim: Boolean) {
        val view = Picture.layout(surfaceWidth, surfaceHeight, aspect, trim)
        glViewport(view.x, view.y, view.width, view.height)
        val edge = Picture.trimFraction(trim)
        vertices.position(0)
        vertices.put(
            floatArrayOf(
                -1f, -1f, 0f, 1f - edge,
                1f, -1f, 1f, 1f - edge,
                -1f, 1f, 0f, edge,
                1f, 1f, 1f, edge,
            )
        )
        vertices.position(0)
    }

    /** Uploads [pixels] (256×240 RGBA) and draws it through [filter]. */
    fun draw(pixels: ByteBuffer, filter: Filter, trim: Boolean) {
        pixels.position(0)
        glActiveTexture(GL_TEXTURE0)
        glBindTexture(GL_TEXTURE_2D, texture)
        glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, Picture.WIDTH, Picture.HEIGHT, GL_RGBA, GL_UNSIGNED_BYTE, pixels)
        // A look that wants a smoothed picture gets one drawn off-screen first,
        // and is then presented from that instead of from the framebuffer.
        val present = if (filter.smooths) smooth() else texture
        glUseProgram(program)
        glActiveTexture(GL_TEXTURE0)
        glBindTexture(GL_TEXTURE_2D, present)
        glUniform1i(uScreen, 0)
        glUniform1i(uKind, filter.id)
        glUniform1f(uCols, Picture.WIDTH.toFloat())
        glUniform1f(uRows, Picture.visibleHeight(trim).toFloat())
        // Where the visible rows sit inside the whole framebuffer, so effects that
        // work in picture space stay put when the edges are trimmed.
        val edge = Picture.trimFraction(trim)
        glUniform2f(uWindow, edge, 1f - 2f * edge)
        quad()
    }

    /**
     * Runs the smoothing chain and hands back the texture the picture should be
     * presented from. Each step doubles the picture and rounds the corners a
     * single NES pixel leaves behind; two steps is enough that the final
     * magnification to the display has curves to stretch rather than stairs.
     *
     * The caller's framebuffer and viewport are asked for rather than tracked,
     * because this runs both on the way to the screen and on the way into the
     * settings preview, and only GL knows which.
     */
    private fun smooth(): Int {
        glGetIntegerv(GL_FRAMEBUFFER_BINDING, boundFrame, 0)
        glGetIntegerv(GL_VIEWPORT, viewport, 0)
        glUseProgram(smoothProgram)
        glUniform1i(uSmoothSrc, 0)
        var source = texture
        for (step in 0 until SMOOTH_STEPS) {
            val scale = 1 shl step
            glBindFramebuffer(GL_FRAMEBUFFER, smoothBuffers[step])
            glViewport(0, 0, Picture.WIDTH * scale * 2, Picture.HEIGHT * scale * 2)
            glActiveTexture(GL_TEXTURE0)
            glBindTexture(GL_TEXTURE_2D, source)
            glUniform2i(uSmoothSize, Picture.WIDTH * scale, Picture.HEIGHT * scale)
            quad()
            source = smoothTextures[step]
        }
        // Let go of the buffer this texture is attached to before rebuilding its
        // mip levels, or the read and the write are the same memory. The glow
        // samples one of those levels rather than spending taps on a blur.
        glBindFramebuffer(GL_FRAMEBUFFER, boundFrame[0])
        glViewport(viewport[0], viewport[1], viewport[2], viewport[3])
        glBindTexture(GL_TEXTURE_2D, source)
        glGenerateMipmap(GL_TEXTURE_2D)
        return source
    }

    private fun quad() {
        vertices.position(0); glVertexAttribPointer(0, 2, GL_FLOAT, false, 16, vertices); glEnableVertexAttribArray(0)
        vertices.position(2); glVertexAttribPointer(1, 2, GL_FLOAT, false, 16, vertices); glEnableVertexAttribArray(1)
        glDrawArrays(GL_TRIANGLE_STRIP, 0, 4)
    }

    /**
     * Draws one still into an off-screen buffer and reads it back, so the settings
     * panel can show the real thing. Leaves the default framebuffer bound; the
     * caller has to put its own viewport back, which [GameSurface] does by marking
     * the picture dirty.
     *
     * Rows come back bottom-up, as GL reads them; the caller flips.
     */
    fun readback(pixels: ByteBuffer, filter: Filter, aspect: Aspect, trim: Boolean, width: Int, height: Int): ByteBuffer {
        if (frame == 0 || frameWidth != width || frameHeight != height) {
            if (frame != 0) { glDeleteFramebuffers(1, intArrayOf(frame), 0); glDeleteTextures(1, intArrayOf(frameTexture), 0) }
            val buffers = IntArray(1); glGenFramebuffers(1, buffers, 0); frame = buffers[0]
            val textures = IntArray(1); glGenTextures(1, textures, 0); frameTexture = textures[0]
            glBindTexture(GL_TEXTURE_2D, frameTexture)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR)
            glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, width, height, 0, GL_RGBA, GL_UNSIGNED_BYTE, null)
            glBindFramebuffer(GL_FRAMEBUFFER, frame)
            glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, frameTexture, 0)
            frameWidth = width; frameHeight = height
            readback = ByteBuffer.allocateDirect(width * height * 4).order(ByteOrder.nativeOrder())
            val status = glCheckFramebufferStatus(GL_FRAMEBUFFER)
            check(status == GL_FRAMEBUFFER_COMPLETE) { "preview buffer incomplete: $status" }
        }
        glBindFramebuffer(GL_FRAMEBUFFER, frame)
        glClearColor(0f, 0f, 0f, 1f); glClear(GL_COLOR_BUFFER_BIT)
        place(width, height, aspect, trim)
        draw(pixels, filter, trim)
        val out = readback!!
        out.position(0)
        glReadPixels(0, 0, width, height, GL_RGBA, GL_UNSIGNED_BYTE, out)
        glBindFramebuffer(GL_FRAMEBUFFER, 0)
        out.position(0)
        return out
    }

    private fun link(fragmentSource: String): Int {
        val vertex = shader(GL_VERTEX_SHADER, VERTEX)
        val fragment = shader(GL_FRAGMENT_SHADER, fragmentSource)
        val id = glCreateProgram()
        glAttachShader(id, vertex); glAttachShader(id, fragment); glLinkProgram(id)
        val linked = IntArray(1); glGetProgramiv(id, GL_LINK_STATUS, linked, 0)
        check(linked[0] != 0) { glGetProgramInfoLog(id) }
        glDeleteShader(vertex); glDeleteShader(fragment)
        return id
    }

    private fun shader(type: Int, source: String): Int {
        val shader = glCreateShader(type)
        glShaderSource(shader, source); glCompileShader(shader)
        val compiled = IntArray(1); glGetShaderiv(shader, GL_COMPILE_STATUS, compiled, 0)
        check(compiled[0] != 0) { glGetShaderInfoLog(shader) }
        return shader
    }

    private companion object {
        /** Doublings of the picture before a smoothing look is presented. Two is 4×. */
        const val SMOOTH_STEPS = 2

        // scale2x, run once per step. Where a single pixel's two neighbours
        // towards one of its corners agree with each other and the opposite pair
        // disagrees, that corner is a stairstep rather than a drawn corner, so it
        // gets cut away. Everything else is left exactly as the console drew it,
        // which is what keeps flat colour flat.
        //
        // Addressing is by fragment coordinate and texelFetch throughout: the
        // comparisons are equality tests, and a filtered or half-texel-off sample
        // would quietly make them all false.
        const val SMOOTH_FRAGMENT =
            "#version 300 es\n" +
                "precision highp float;\n" +
                "precision highp int;\n" +
                "uniform sampler2D src; uniform ivec2 srcSize;\n" +
                "out vec4 color;\n" +
                "vec3 at(ivec2 p){ return texelFetch(src, clamp(p, ivec2(0), srcSize - 1), 0).rgb; }\n" +
                "void main(){\n" +
                "  ivec2 o = ivec2(gl_FragCoord.xy);\n" +
                "  ivec2 c = o / 2;\n" +          // the source texel this output sits in
                "  ivec2 q = o - c * 2;\n" +      // and which of its four quarters
                "  vec3 e = at(c);\n" +
                "  vec3 up = at(c + ivec2(0, -1)), down = at(c + ivec2(0, 1));\n" +
                "  vec3 left = at(c + ivec2(-1, 0)), right = at(c + ivec2(1, 0));\n" +
                "  vec3 result = e;\n" +
                // Inside a solid run both pairs match and nothing is cut.
                "  if (up != down && left != right) {\n" +
                "    vec3 v = q.y == 0 ? up : down;\n" +
                "    vec3 h = q.x == 0 ? left : right;\n" +
                "    if (v == h) result = v;\n" +
                "  }\n" +
                "  color = vec4(result, 1.0);\n" +
                "}"

        const val VERTEX =
            "#version 300 es\n" +
                "layout(location=0) in vec2 p; layout(location=1) in vec2 uv; out vec2 tex;" +
                "void main(){ gl_Position = vec4(p, 0.0, 1.0); tex = uv; }"

        // One shader, one branch per look. Everything works in "picture space" -
        // x and y both running 0..1 across the visible area - so a filter behaves
        // the same whether or not the overscan rows are trimmed away.
        const val FRAGMENT =
            "#version 300 es\n" +
                "precision mediump float;\n" +
                "in vec2 tex; uniform sampler2D screen; uniform int kind;\n" +
                "uniform float cols; uniform float rows; uniform vec2 window;\n" +
                "out vec4 color;\n" +
                "const float PI = 3.14159265;\n" +
                "float luma(vec3 c){ return dot(c, vec3(0.299, 0.587, 0.114)); }\n" +
                // Between the quad's texture coordinates and the visible picture.
                "vec2 toPicture(vec2 uv){ return vec2(uv.x, (uv.y - window.x) / window.y); }\n" +
                "vec2 toTexture(vec2 p){ return vec2(p.x, window.x + p.y * window.y); }\n" +
                "vec3 pick(vec2 p){ return texture(screen, toTexture(p)).rgb; }\n" +
                "void main(){\n" +
                "  vec2 p = toPicture(tex);\n" +
                "  float edge = 1.0;\n" +
                // Old TV bends the picture over a tube and darkens the corners.
                // Outside the glass is black, not a smeared edge pixel.
                "  if (kind == 2) {\n" +
                "    vec2 c = p * 2.0 - 1.0;\n" +
                "    c *= 1.0 + 0.055 * dot(c, c);\n" +
                "    if (abs(c.x) > 1.0 || abs(c.y) > 1.0) { color = vec4(0.0, 0.0, 0.0, 1.0); return; }\n" +
                "    p = c * 0.5 + 0.5;\n" +
                "    edge = 1.0 - 0.28 * dot(c, c);\n" +
                "  }\n" +
                "  vec3 rgb = pick(p);\n" +
                "  float band = sin(PI * p.y * rows);\n" +
                "  if (kind == 1) {\n" +
                "    rgb *= 0.55 + 0.45 * band * band;\n" +
                "  } else if (kind == 2) {\n" +
                "    rgb *= 0.62 + 0.38 * band * band;\n" +
                // An aperture grille is a property of the glass, not of the game,
                // so its stripes are counted in device pixels.
                "    int stripe = int(mod(gl_FragCoord.x, 3.0));\n" +
                "    vec3 grille = stripe == 0 ? vec3(1.0, 0.72, 0.72)\n" +
                "                : stripe == 1 ? vec3(0.72, 1.0, 0.72) : vec3(0.72, 0.72, 1.0);\n" +
                "    rgb *= grille * edge;\n" +
                "  } else if (kind == 3) {\n" +
                // A gap around every console pixel, both axes, like an LCD grid.
                "    vec2 cell = fract(vec2(p.x * cols, p.y * rows));\n" +
                "    vec2 gap = smoothstep(0.0, 0.18, cell) * smoothstep(0.0, 0.18, 1.0 - cell);\n" +
                "    rgb *= mix(0.42, 1.0, min(gap.x, gap.y));\n" +
                "    rgb = mix(rgb, vec3(luma(rgb)) * vec3(0.92, 0.98, 0.9), 0.25);\n" +
                "  } else if (kind == 4) {\n" +
                "    float l = luma(rgb);\n" +
                "    vec3 dark = vec3(0.059, 0.220, 0.059), mid = vec3(0.188, 0.384, 0.188);\n" +
                "    vec3 light = vec3(0.545, 0.675, 0.059), pale = vec3(0.608, 0.737, 0.059);\n" +
                "    rgb = l < 0.25 ? dark : l < 0.5 ? mid : l < 0.75 ? light : pale;\n" +
                "  } else if (kind == 6) {\n" +
                "    float l = luma(rgb);\n" +
                "    rgb = clamp(vec3(l * 1.07 + 0.06, l * 0.94 + 0.03, l * 0.72), 0.0, 1.0);\n" +
                "  } else if (kind == 7) {\n" +
                // Cheap bloom: the brightest neighbour bleeds in, then the colour
                // is pushed away from grey.
                "    vec2 texel = vec2(1.0 / cols, 1.0 / rows);\n" +
                "    vec3 glow = max(max(pick(p + vec2(texel.x, 0.0)), pick(p - vec2(texel.x, 0.0))),\n" +
                "                    max(pick(p + vec2(0.0, texel.y)), pick(p - vec2(0.0, texel.y))));\n" +
                "    rgb += 0.40 * glow * glow;\n" +
                "    float l = luma(rgb);\n" +
                "    rgb = clamp(vec3(l) + (rgb - vec3(l)) * 1.7, 0.0, 1.0);\n" +
                "  } else if (kind == 8) {\n" +
                // Cartoon. The picture has already been smoothed into curves off
                // screen, so what is left is the part that makes it drawn rather
                // than rendered: flatten the shading into bands, push the colour,
                // and lay an ink line along the edges.
                //
                // The ink is a dark tint of the colour it runs through rather than
                // black, which is what stops a bright sprite from looking like it
                // was cut out and pasted down.
                // Half a console pixel out, which on the 4× buffer is two texels.
                "    vec2 rad = vec2(0.5 / cols, 0.5 / rows);\n" +
                "    vec3 a = pick(p - vec2(0.0, rad.y)), b = pick(p + vec2(0.0, rad.y));\n" +
                "    vec3 c = pick(p - vec2(rad.x, 0.0)), d = pick(p + vec2(rad.x, 0.0));\n" +
                // Opposite pairs rather than centre-to-neighbour, so the line sits
                // on the edge instead of along one side of it.
                "    float e = max(length(a - b), length(c - d));\n" +
                "    float ink = min(0.86, 0.50 * smoothstep(0.24, 0.34, e) + 0.40 * smoothstep(0.50, 0.62, e));\n" +
                // No posterising step, deliberately. Cel shading normally has to
                // flatten a rendered image into bands, but a 2C02 frame arrives
                // flat already: four colours to a tile, chosen by hand. Banding it
                // again only walks those colours onto a quantiser's grid and loses
                // what the artist picked. All this wants is a little more paint.
                "    float f = luma(rgb);\n" +
                "    rgb = vec3(f) + (rgb - vec3(f)) * 1.20;\n" +
                "    rgb = (rgb - 0.5) * 1.06 + 0.5;\n" +
                "    rgb *= mix(1.0, 0.13, ink);\n" +
                // A small mip level is a blur somebody else already paid for.
                "    vec3 glow = textureLod(screen, toTexture(p), 4.0).rgb;\n" +
                "    rgb = clamp(rgb + 0.30 * glow * glow, 0.0, 1.0);\n" +
                "  }\n" +
                "  if (kind != 2) rgb *= edge;\n" +
                "  color = vec4(rgb, 1.0);\n" +
                "}"
    }
}
