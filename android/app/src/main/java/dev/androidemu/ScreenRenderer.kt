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

    val ready get() = program != 0

    fun create() {
        val vertex = shader(GL_VERTEX_SHADER, VERTEX)
        val fragment = shader(GL_FRAGMENT_SHADER, FRAGMENT)
        program = glCreateProgram(); glAttachShader(program, vertex); glAttachShader(program, fragment); glLinkProgram(program)
        val linked = IntArray(1); glGetProgramiv(program, GL_LINK_STATUS, linked, 0)
        check(linked[0] != 0) { glGetProgramInfoLog(program) }
        glDeleteShader(vertex); glDeleteShader(fragment)
        uScreen = glGetUniformLocation(program, "screen")
        uKind = glGetUniformLocation(program, "kind")
        uCols = glGetUniformLocation(program, "cols")
        uRows = glGetUniformLocation(program, "rows")
        uWindow = glGetUniformLocation(program, "window")
        val textures = IntArray(1); glGenTextures(1, textures, 0); texture = textures[0]
        glBindTexture(GL_TEXTURE_2D, texture)
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST)
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST)
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE)
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE)
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, Picture.WIDTH, Picture.HEIGHT, 0, GL_RGBA, GL_UNSIGNED_BYTE, null)
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
        glUseProgram(program)
        glActiveTexture(GL_TEXTURE0)
        glBindTexture(GL_TEXTURE_2D, texture)
        glUniform1i(uScreen, 0)
        glUniform1i(uKind, filter.ordinal)
        glUniform1f(uCols, Picture.WIDTH.toFloat())
        glUniform1f(uRows, Picture.visibleHeight(trim).toFloat())
        // Where the visible rows sit inside the whole framebuffer, so effects that
        // work in picture space stay put when the edges are trimmed.
        val edge = Picture.trimFraction(trim)
        glUniform2f(uWindow, edge, 1f - 2f * edge)
        glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, Picture.WIDTH, Picture.HEIGHT, GL_RGBA, GL_UNSIGNED_BYTE, pixels)
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

    private fun shader(type: Int, source: String): Int {
        val shader = glCreateShader(type)
        glShaderSource(shader, source); glCompileShader(shader)
        val compiled = IntArray(1); glGetShaderiv(shader, GL_COMPILE_STATUS, compiled, 0)
        check(compiled[0] != 0) { glGetShaderInfoLog(shader) }
        return shader
    }

    private companion object {
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
                "  } else if (kind == 5) {\n" +
                "    float l = luma(rgb);\n" +
                "    rgb = vec3(clamp((l - 0.5) * 1.15 + 0.5, 0.0, 1.0));\n" +
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
                "  }\n" +
                "  if (kind != 2) rgb *= edge;\n" +
                "  color = vec4(rgb, 1.0);\n" +
                "}"
    }
}
