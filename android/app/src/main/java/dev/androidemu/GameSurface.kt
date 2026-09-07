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
    private val vertices = ByteBuffer.allocateDirect(16 * 4).order(ByteOrder.nativeOrder()).asFloatBuffer().apply {
        put(floatArrayOf(-1f,-1f,0f,1f, 1f,-1f,1f,1f, -1f,1f,0f,0f, 1f,1f,1f,0f)); position(0)
    }
    private var program = 0
    private var texture = 0
    @Volatile var playing = false
    @Volatile var rewinding = false
    private var exhausted = false
    @Volatile var loaded = false
    private var nextFrame = 0L
    @Volatile private var frameRate = 60.0988f
    fun setGameFrameRate(rate: Float) { frameRate = rate; post { if (Build.VERSION.SDK_INT >= 30 && holder.surface.isValid) holder.surface.setFrameRate(rate, Surface.FRAME_RATE_COMPATIBILITY_FIXED_SOURCE) } }
    private var attached = false
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
            val fragment = shader(GL_FRAGMENT_SHADER, "#version 300 es\nprecision mediump float; in vec2 tex; uniform sampler2D screen; out vec4 color; void main(){color=texture(screen,tex);}")
            program = glCreateProgram(); glAttachShader(program, vertex); glAttachShader(program, fragment); glLinkProgram(program)
            val result = IntArray(1); glGetProgramiv(program, GL_LINK_STATUS, result, 0); check(result[0] != 0) { glGetProgramInfoLog(program) }
            glDeleteShader(vertex); glDeleteShader(fragment)
            val textures = IntArray(1); glGenTextures(1, textures, 0); texture = textures[0]; glBindTexture(GL_TEXTURE_2D, texture)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST); glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST)
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE); glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE)
            glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, 256, 240, 0, GL_RGBA, GL_UNSIGNED_BYTE, null)
        } catch (e: Exception) { post { failure(e.message ?: "Video could not start") } }
    }
    override fun onSurfaceChanged(gl: GL10?, width: Int, height: Int) {
        val w = minOf(width, height * 4 / 3); val h = w * 3 / 4
        glViewport((width-w)/2, (height-h)/2, w, h)
        if (Build.VERSION.SDK_INT >= 30) holder.surface.setFrameRate(frameRate, Surface.FRAME_RATE_COMPATIBILITY_FIXED_SOURCE)
    }
    override fun onDrawFrame(gl: GL10?) {
        glClearColor(0f,0f,0f,1f); glClear(GL_COLOR_BUFFER_BIT)
        if (!loaded || program == 0) return
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
                Native.frame(pixels, p1, p2, playing)
            }
            pixels.position(0); glUseProgram(program); glActiveTexture(GL_TEXTURE0); glBindTexture(GL_TEXTURE_2D, texture)
            glUniform1i(glGetUniformLocation(program, "screen"), 0)
            glTexSubImage2D(GL_TEXTURE_2D, 0, 0,0,256,240,GL_RGBA,GL_UNSIGNED_BYTE,pixels)
            vertices.position(0); glVertexAttribPointer(0,2,GL_FLOAT,false,16,vertices); glEnableVertexAttribArray(0)
            vertices.position(2); glVertexAttribPointer(1,2,GL_FLOAT,false,16,vertices); glEnableVertexAttribArray(1)
            glDrawArrays(GL_TRIANGLE_STRIP,0,4)
        } catch (e: Exception) { playing = false; post { failure(e.message ?: "This game stopped") } }
    }
}
