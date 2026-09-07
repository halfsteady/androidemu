package dev.androidemu

import android.content.Context
import android.opengl.GLES30.*
import android.opengl.GLSurfaceView
import android.os.Build
import android.view.Choreographer
import android.view.Surface
import java.nio.ByteBuffer
import java.nio.ByteOrder
import javax.microedition.khronos.egl.EGLConfig
import javax.microedition.khronos.opengles.GL10

/** UI vsync schedules work on the GL thread; native emulation never blocks Compose. */
class GameSurface(context: Context, private val inputs: () -> Pair<Int, Int>, private val rewindEnded: () -> Unit, private val failure: (String) -> Unit) : GLSurfaceView(context), GLSurfaceView.Renderer, Choreographer.FrameCallback {
    private val pixels = ByteBuffer.allocateDirect(256 * 240 * 4)
    private val vertices = ByteBuffer.allocateDirect(16 * 4).order(ByteOrder.nativeOrder()).asFloatBuffer()
    private var program = 0
    private var texture = 0
    @Volatile var playing = false
    @Volatile var rewinding = false
    /** Frames to run per displayed frame while fast-forward is held; 1 is normal play. */
    @Volatile var speed = 1
    private var exhausted = false
    @Volatile var loaded = false
    private var nextFrame = 0L
    @Volatile private var frameRate = 60.0988f
    fun setGameFrameRate(rate: Float) { frameRate = rate; post { if (Build.VERSION.SDK_INT >= 30 && holder.surface.isValid) holder.surface.setFrameRate(rate, Surface.FRAME_RATE_COMPATIBILITY_FIXED_SOURCE) } }
    private var attached = false

    // Presentation, set from the UI thread and read on the GL thread. Applied at
    // the top of a draw rather than when it changes, because the viewport and the
    // vertex buffer belong to the GL context.
    @Volatile private var aspect = Aspect.Television
    @Volatile private var trimEdges = false
    @Volatile private var scanlines = false
    @Volatile private var pictureDirty = true
    private var surfaceWidth = 0
    private var surfaceHeight = 0
    fun setPicture(aspect: Aspect, trimEdges: Boolean, scanlines: Boolean) {
        this.aspect = aspect; this.trimEdges = trimEdges; this.scanlines = scanlines
        pictureDirty = true
        requestRender()
    }

    init { setEGLContextClientVersion(3); preserveEGLContextOnPause = true; setRenderer(this); renderMode = RENDERMODE_WHEN_DIRTY }
    override fun onAttachedToWindow() { super.onAttachedToWindow(); attached = true; Choreographer.getInstance().postFrameCallback(this) }
    override fun onDetachedFromWindow() { attached = false; Choreographer.getInstance().removeFrameCallback(this); super.onDetachedFromWindow() }
    override fun doFrame(time: Long) {
        if (playing && loaded) {
            if (nextFrame == 0L || time - nextFrame > 100_000_000L) nextFrame = time
            if (time >= nextFrame) { nextFrame += (1_000_000_000.0 / frameRate).toLong(); requestRender() }
        } else nextFrame = 0L
        if (attached) Choreographer.getInstance().postFrameCallback(this)
    }
    fun task(action: () -> Unit) { queueEvent { try { action() } catch (e: Exception) { playing = false; post { failure(e.message ?: "Something went wrong") } } } }
    override fun onSurfaceCreated(gl: GL10?, config: EGLConfig?) {
        fun shader(type: Int, source: String): Int {
            val shader = glCreateShader(type); glShaderSource(shader, source); glCompileShader(shader)
            val result = IntArray(1); glGetShaderiv(shader, GL_COMPILE_STATUS, result, 0)
            check(result[0] != 0) { glGetShaderInfoLog(shader) }; return shader
        }
        try {
            val vertex = shader(GL_VERTEX_SHADER, "#version 300 es\nlayout(location=0) in vec2 p; layout(location=1) in vec2 uv; out vec2 tex; void main(){gl_Position=vec4(p,0,1);tex=uv;}")
            // One dark band per source row, phased so row centres stay at full
            // brightness. Smooth rather than a hard step, so it doesn't alias
            // into moiré at scales that aren't whole numbers.
            val fragment = shader(
                GL_FRAGMENT_SHADER,
                "#version 300 es\nprecision mediump float; in vec2 tex; uniform sampler2D screen;" +
                    " uniform float scanlines; uniform float rows; out vec4 color;" +
                    " void main(){ float band = sin(3.14159265 * tex.y * rows);" +
                    " float shade = mix(1.0, 0.55 + 0.45 * band * band, scanlines);" +
                    " color = vec4(texture(screen, tex).rgb * shade, 1.0); }",
            )
            program = glCreateProgram(); glAttachShader(program, vertex); glAttachShader(program, fragment); glLinkProgram(program)
            val result = IntArray(1); glGetProgramiv(program, GL_LINK_STATUS, result, 0); check(result[0] != 0) { glGetProgramInfoLog(program) }
            glDeleteShader(vertex); glDeleteShader(fragment)
            val textures = IntArray(1); glGenTextures(1, textures, 0); texture = textures[0]; glBindTexture(GL_TEXTURE_2D, texture)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST); glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE); glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE)
            glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, 256, 240, 0, GL_RGBA, GL_UNSIGNED_BYTE, null)
            pictureDirty = true
        } catch (e: Exception) { post { failure(e.message ?: "Video could not start") } }
    }
    override fun onSurfaceChanged(gl: GL10?, width: Int, height: Int) {
        surfaceWidth = width; surfaceHeight = height
        applyPicture()
        if (Build.VERSION.SDK_INT >= 30) holder.surface.setFrameRate(frameRate, Surface.FRAME_RATE_COMPATIBILITY_FIXED_SOURCE)
    }
    /** Viewport and texture coordinates for the current aspect and trim. */
    private fun applyPicture() {
        val view = Picture.layout(surfaceWidth, surfaceHeight, aspect, trimEdges)
        glViewport(view.x, view.y, view.width, view.height)
        // The framebuffer is uploaded whole; trimming moves the window the quad
        // samples, which keeps the upload a single unconditional glTexSubImage2D.
        // Texture rows run top-down while the quad runs bottom-up, so the top of
        // the picture is v = trim and the bottom is 1 - trim.
        val trim = Picture.trimFraction(trimEdges)
        vertices.position(0)
        vertices.put(
            floatArrayOf(
                -1f, -1f, 0f, 1f - trim,
                1f, -1f, 1f, 1f - trim,
                -1f, 1f, 0f, trim,
                1f, 1f, 1f, trim,
            )
        )
        vertices.position(0)
        pictureDirty = false
    }
    override fun onDrawFrame(gl: GL10?) {
        glClearColor(0f,0f,0f,1f); glClear(GL_COLOR_BUFFER_BIT)
        if (!loaded || program == 0) return
        if (pictureDirty) applyPicture()
        try {
            val (p1, p2) = inputs()
            // Rewinding replaces the frame rather than following it. Once the
            // chain runs dry the picture holds on its oldest frame - releasing
            // the button is what resumes play, so a held button never quietly
            // turns back into forward motion.
            if (rewinding) {
                if (!Native.rewind(pixels) && !exhausted) { exhausted = true; post { rewindEnded() } }
            } else {
                exhausted = false
                // Fast-forward runs whole frames and shows the last one. The
                // input applies to every frame in the batch, which is what makes
                // holding a direction through a fast-forward behave sensibly.
                val frames = if (playing && speed > 1) speed else 1
                repeat(frames) { Native.frame(pixels, p1, p2, playing) }
            }
            pixels.position(0); glUseProgram(program); glActiveTexture(GL_TEXTURE0); glBindTexture(GL_TEXTURE_2D, texture)
            glUniform1i(glGetUniformLocation(program, "screen"), 0)
            glUniform1f(glGetUniformLocation(program, "scanlines"), if (scanlines) 1f else 0f)
            // Texture coordinates stay in full-framebuffer space even when the
            // edges are trimmed, so this is always the framebuffer's row count.
            glUniform1f(glGetUniformLocation(program, "rows"), Picture.HEIGHT.toFloat())
            glTexSubImage2D(GL_TEXTURE_2D, 0, 0,0,256,240,GL_RGBA,GL_UNSIGNED_BYTE,pixels)
            vertices.position(0); glVertexAttribPointer(0,2,GL_FLOAT,false,16,vertices); glEnableVertexAttribArray(0)
            vertices.position(2); glVertexAttribPointer(1,2,GL_FLOAT,false,16,vertices); glEnableVertexAttribArray(1)
            glDrawArrays(GL_TRIANGLE_STRIP,0,4)
        } catch (e: Exception) { playing = false; post { failure(e.message ?: "This game stopped") } }
    }
}
