package dev.androidemu

import android.content.Context
import android.opengl.GLES30.*
import android.opengl.GLSurfaceView
import android.os.Build
import android.view.Choreographer
import android.view.Surface
import java.nio.ByteBuffer
import javax.microedition.khronos.egl.EGLConfig
import javax.microedition.khronos.opengles.GL10

/** UI vsync schedules work on the GL thread; native emulation never blocks Compose. */
class GameSurface(context: Context, private val inputs: () -> Pair<Int, Int>, private val rewindEnded: () -> Unit, private val failure: (String) -> Unit) : GLSurfaceView(context), GLSurfaceView.Renderer, Choreographer.FrameCallback {
    private val pixels = ByteBuffer.allocateDirect(Picture.WIDTH * Picture.HEIGHT * 4)
    private val screen = ScreenRenderer()
    @Volatile var playing = false
    /**
     * How time is running: 0 is ordinary play, a positive value is that many
     * emulated frames per displayed frame, and a negative value is that many steps
     * back through the rewind chain. One field, because it is one control.
     */
    @Volatile var scrub = 0
    /** Frames available to rewind through, refreshed as a side effect of drawing. */
    @Volatile var depth = 0
    private var exhausted = false
    @Volatile var loaded = false
    private var nextFrame = 0L
    @Volatile private var frameRate = 60.0988f
    fun setGameFrameRate(rate: Float) { frameRate = rate; post { if (Build.VERSION.SDK_INT >= 30 && holder.surface.isValid) holder.surface.setFrameRate(rate, Surface.FRAME_RATE_COMPATIBILITY_FIXED_SOURCE) } }
    /** Seconds of play a frame count is worth, for the skip-back buttons. */
    fun framesFor(seconds: Int) = (seconds * frameRate).toInt()
    private var attached = false

    // Presentation, set from the UI thread and read on the GL thread. Applied at
    // the top of a draw rather than when it changes, because the viewport and the
    // vertex buffer belong to the GL context.
    @Volatile private var aspect = Aspect.Television
    @Volatile private var trimEdges = false
    @Volatile private var filter = Filter.None
    @Volatile private var pictureDirty = true
    private var surfaceWidth = 0
    private var surfaceHeight = 0
    fun setPicture(aspect: Aspect, trimEdges: Boolean, filter: Filter) {
        this.aspect = aspect; this.trimEdges = trimEdges; this.filter = filter
        pictureDirty = true
        requestRender()
    }

    init { setEGLContextClientVersion(3); preserveEGLContextOnPause = true; setRenderer(this); renderMode = RENDERMODE_WHEN_DIRTY }
    override fun onAttachedToWindow() { super.onAttachedToWindow(); attached = true; Choreographer.getInstance().postFrameCallback(this) }
    override fun onDetachedFromWindow() { attached = false; Choreographer.getInstance().removeFrameCallback(this); super.onDetachedFromWindow() }
    override fun doFrame(time: Long) {
        // Rewinding is driven by the display rather than by the game clock: there
        // is no emulation to pace, just frames to walk back through.
        if ((playing || scrub < 0) && loaded) {
            if (nextFrame == 0L || time - nextFrame > 100_000_000L) nextFrame = time
            if (time >= nextFrame) { nextFrame += (1_000_000_000.0 / frameRate).toLong(); requestRender() }
        } else nextFrame = 0L
        if (attached) Choreographer.getInstance().postFrameCallback(this)
    }
    fun task(action: () -> Unit) { queueEvent { try { action() } catch (e: Exception) { playing = false; post { failure(e.message ?: "Something went wrong") } } } }
    override fun onSurfaceCreated(gl: GL10?, config: EGLConfig?) {
        try { screen.create(); pictureDirty = true }
        catch (e: Exception) { post { failure(e.message ?: "Video could not start") } }
    }
    override fun onSurfaceChanged(gl: GL10?, width: Int, height: Int) {
        surfaceWidth = width; surfaceHeight = height
        if (screen.ready) screen.place(width, height, aspect, trimEdges)
        pictureDirty = false
        if (Build.VERSION.SDK_INT >= 30) holder.surface.setFrameRate(frameRate, Surface.FRAME_RATE_COMPATIBILITY_FIXED_SOURCE)
    }
    override fun onDrawFrame(gl: GL10?) {
        glClearColor(0f, 0f, 0f, 1f); glClear(GL_COLOR_BUFFER_BIT)
        if (!loaded || !screen.ready) return
        if (pictureDirty) { screen.place(surfaceWidth, surfaceHeight, aspect, trimEdges); pictureDirty = false }
        try {
            val (p1, p2) = inputs()
            val speed = scrub
            // Rewinding replaces the frame rather than following it. Once the chain
            // runs dry the picture holds on its oldest frame — returning the control
            // to the centre is what resumes play, so a held direction never quietly
            // turns back into forward motion.
            if (speed < 0) {
                var stepped = 0
                while (stepped < -speed) {
                    if (!Native.rewind(pixels)) {
                        if (!exhausted) { exhausted = true; post { rewindEnded() } }
                        break
                    }
                    stepped++
                }
            } else {
                exhausted = false
                // Fast-forward runs whole frames and shows the last one. The input
                // applies to every frame in the batch, which is what makes holding
                // a direction through a fast-forward behave sensibly.
                val frames = if (playing && speed > 1) speed else 1
                repeat(frames) { Native.frame(pixels, p1, p2, playing) }
            }
            depth = Native.rewindDepth()
            screen.draw(pixels, filter, trimEdges)
        } catch (e: Exception) { playing = false; post { failure(e.message ?: "This game stopped") } }
    }

    /**
     * Jumps back up to [frames] and repaints, reporting how far it actually got.
     * Separate from the scrub track because a fixed jump is a different gesture:
     * one tap, a known distance, no holding.
     */
    fun skipBack(frames: Int, then: (Int) -> Unit) {
        task {
            var stepped = 0
            while (stepped < frames && Native.rewind(pixels)) stepped++
            depth = Native.rewindDepth()
            exhausted = false
            requestRender()
            post { then(stepped) }
        }
    }

    /**
     * Renders one still through the current shader into an off-screen buffer and
     * hands back the pixels, so the settings panel can show the real filter rather
     * than an approximation of it. Bottom-up, as GL reads them.
     *
     * The pixels are copied out: the renderer reuses its readback buffer, and the
     * caller receives this on the main thread after the GL thread has moved on.
     */
    fun preview(sample: ByteBuffer, filter: Filter, aspect: Aspect, trim: Boolean, width: Int, height: Int, then: (ByteArray) -> Unit) {
        task {
            if (!screen.ready) return@task
            val read = screen.readback(sample, filter, aspect, trim, width, height)
            val out = ByteArray(width * height * 4)
            read.position(0); read.get(out)
            // The viewport and quad now belong to the preview, so put the game's
            // back before the next draw.
            pictureDirty = true
            requestRender()
            post { then(out) }
        }
    }
}
