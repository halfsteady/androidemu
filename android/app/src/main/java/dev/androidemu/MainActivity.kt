package dev.androidemu

import android.annotation.SuppressLint
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import android.content.BroadcastReceiver
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.os.Bundle
import android.os.SystemClock
import android.provider.MediaStore
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.delay
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.text.DateFormat
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.math.roundToInt

class MainActivity : ComponentActivity() {
    private lateinit var library: Library
    private lateinit var surface: GameSurface
    private lateinit var input: ControllerInput
    private lateinit var settings: Settings
    private var games by mutableStateOf(emptyList<Game>())
    private var game by mutableStateOf<Game?>(null)
    private var paused by mutableStateOf(true)
    private var busy by mutableStateOf(false)
    private var message by mutableStateOf<String?>(null)
    private var slots by mutableStateOf(emptyList<SaveSlot>())
    private var showSlots by mutableStateOf(false)
    private var showSettings by mutableStateOf(false)
    private var showProblems by mutableStateOf(false)
    // Bumped when a cover file is replaced, so tiles re-read it. Game rows compare
    // equal across a reload, and a lazy grid is entitled to skip an equal item.
    private var covers by mutableIntStateOf(0)
    private var artFor: Game? = null
    /**
     * Where the time control is: 0 is ordinary play, negative runs the game
     * backwards that many frames per displayed frame, positive runs it forward.
     * One value, because rewind and fast-forward are one control.
     */
    private var scrub by mutableIntStateOf(0)
    private var rewindAtStart by mutableStateOf(false)
    /** Frames the rewind chain holds, polled while a game is open. */
    private var rewindDepth by mutableIntStateOf(0)
    private fun applyScrub(speed: Int) {
        if (scrub == speed || game == null) return
        val wasSilent = scrub != 0
        scrub = speed
        // Rewinding must not carry a held button into the past. Fast-forward must
        // carry it, or holding a direction while skipping ahead does nothing.
        if (speed < 0) { rewindAtStart = false; input.clear() }
        surface.scrub = speed
        // Neither direction plays new audio, so the stream stops rather than
        // draining into an underrun or chipmunking along at four times the pitch.
        // Only the crossing matters: dragging changes the speed constantly.
        if (wasSilent != (speed != 0)) surface.task { Native.audio(speed == 0 && surface.playing && !backgrounded) }
    }
    /**
     * A fixed jump back, which is a different gesture from the track: one tap, a
     * known distance, no holding. Greyed out by the caller when the chain is too
     * short to honour it.
     */
    private fun skipBack(seconds: Int) {
        if (game == null || busy) return
        rewindAtStart = false
        surface.skipBack(surface.framesFor(seconds)) { stepped ->
            rewindDepth = surface.depth
            if (stepped == 0) rewindAtStart = true
        }
    }
    private var fullscreen by mutableStateOf(false)
    private var fullscreenTouch by mutableStateOf(false)
    private var mapping by mutableStateOf(false)
    private var mappingStep by mutableStateOf(0)
    private var mappingName by mutableStateOf("")
    private val mappedKeys = mutableListOf<Pair<KeyEvent, Int>>()
    private val mapButtons = listOf("A" to 1, "B" to 2, "Select" to 4, "Start" to 8)
    private fun applyFullscreen(value: Boolean) {
        fullscreen = value
        val bars = WindowCompat.getInsetsController(window, window.decorView)
        bars.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (value) bars.hide(WindowInsetsCompat.Type.systemBars()) else bars.show(WindowInsetsCompat.Type.systemBars())
    }
    private fun startMapping() { if (game != null && !paused) pause(); mappedKeys.clear(); mappingStep = 0; mappingName = ""; mapping = true; input.clear() }
    private var overwrite by mutableStateOf<Int?>(null)
    // What the settings preview draws, and the result. The sample is a whole
    // framebuffer; the image is that framebuffer through the current shader.
    private var previewSample by mutableStateOf<ByteBuffer?>(null)
    private var previewImage by mutableStateOf<ImageBitmap?>(null)
    /**
     * Opens settings and lines up something to preview: the frame the game is
     * sitting on, else the newest saved moment on the shelf, else a built pattern
     * chosen to show what the filters do.
     */
    private fun openSettings() {
        previewImage = null
        showSettings = true
        if (game != null && surface.loaded) {
            surface.task {
                Native.frame(shot, 0, 0, false)
                val copy = ByteBuffer.allocateDirect(shot.capacity()).order(ByteOrder.nativeOrder())
                shot.position(0); copy.put(shot); copy.position(0); shot.position(0)
                runOnUiThread { previewSample = copy }
            }
        } else {
            previewSample = savedMoment() ?: framebufferOf(SampleFrame.pixels())
        }
    }
    /** Lets go of the preview buffers; the next open captures a fresh sample. */
    private fun closeSettings() { showSettings = false; previewImage = null; previewSample = null }
    private fun framebufferOf(pixels: ByteArray): ByteBuffer =
        ByteBuffer.allocateDirect(pixels.size).order(ByteOrder.nativeOrder()).put(pixels).also { it.position(0) }
    /** The newest thumbnail on the shelf, scaled to a framebuffer. */
    private fun savedMoment(): ByteBuffer? {
        val file = games.asSequence().map { library.thumbnail(it, -1) }.firstOrNull { it.exists() } ?: return null
        val decoded = runCatching { BitmapFactory.decodeFile(file.path) }.getOrNull() ?: return null
        val scaled = if (decoded.width == Picture.WIDTH && decoded.height == Picture.HEIGHT) decoded
            else Bitmap.createScaledBitmap(decoded, Picture.WIDTH, Picture.HEIGHT, true)
        val out = ByteBuffer.allocateDirect(Picture.WIDTH * Picture.HEIGHT * 4).order(ByteOrder.nativeOrder())
        scaled.copyPixelsToBuffer(out)
        if (scaled !== decoded) scaled.recycle()
        decoded.recycle()
        out.position(0)
        return out
    }
    /**
     * Draws the sample through the real shader off-screen and keeps the result.
     * The preview box is 4:3, so a narrower shape shows its own side bars — which
     * is the difference worth seeing.
     */
    private fun renderPreview() {
        val sample = previewSample ?: return
        surface.preview(sample, settings.filter, settings.aspect, settings.trimEdges, PREVIEW_WIDTH, PREVIEW_HEIGHT) { bytes ->
            val raw = Bitmap.createBitmap(PREVIEW_WIDTH, PREVIEW_HEIGHT, Bitmap.Config.ARGB_8888)
            raw.copyPixelsFromBuffer(ByteBuffer.wrap(bytes))
            // GL reads rows bottom-up.
            val upright = Bitmap.createBitmap(raw, 0, 0, PREVIEW_WIDTH, PREVIEW_HEIGHT, Matrix().apply { postScale(1f, -1f) }, false)
            raw.recycle()
            previewImage = upright.asImageBitmap()
        }
    }
    @Volatile private var backgrounded = false
    @Volatile private var activeGame: Game? = null
    // Elapsed-time clock for playtime. Wall clock would count a paused game if
    // the timezone or the system clock moved under it.
    @Volatile private var playingSince = 0L
    private fun elapsedSeconds(): Long {
        val since = playingSince
        playingSince = 0L
        return if (since == 0L) 0 else (SystemClock.elapsedRealtime() - since) / 1000
    }
    // The failure a player sees, plus the real reason kept where it can be read
    // back later. A message that only ever appears once in a dialog is a message
    // nobody can act on afterwards.
    private fun report(plain: String, detail: String?) {
        val real = detail?.takeIf { it.isNotBlank() }
        library.logProblem(plain, real ?: "no further detail")
        message = if (real == null) plain else "$plain\n\n$real"
    }
    private val importGame = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) work("Adding game", "That game file didn't work.") {
            val (title, bytes) = library.readImport(uri)
            surface.loaded = false
            try {
                val id = Native.load(bytes)
                val selected = library.add(id, title, bytes)
                openLoaded(selected)
            } catch (e: Exception) { runOnUiThread { game = null }; throw e }
        }
    }
    // Box art is a picture chosen by hand, downscaled on the way in so the shelf
    // never decodes a 12-megapixel photo to fill a 240 dp tile.
    private val importArt = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        val selected = artFor
        artFor = null
        if (uri == null || selected == null) return@registerForActivityResult
        try {
            val bytes = library.readBytes(uri)
            val decoded = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                ?: error("That file isn't a picture this app can read")
            val scale = minOf(1f, ART_MAX_EDGE / maxOf(decoded.width, decoded.height).toFloat())
            val art = if (scale < 1f) Bitmap.createScaledBitmap(decoded, (decoded.width * scale).toInt().coerceAtLeast(1), (decoded.height * scale).toInt().coerceAtLeast(1), true) else decoded
            val out = ByteArrayOutputStream()
            art.compress(Bitmap.CompressFormat.PNG, 100, out)
            if (art !== decoded) art.recycle()
            decoded.recycle()
            Library.atomic(library.art(selected), out.toByteArray())
            covers++
            message = "Box art set for ${selected.title}."
        } catch (e: Exception) { report("That picture couldn't be used as box art.", e.message) }
    }
    private fun clearArt(selected: Game) {
        if (library.art(selected).delete()) { covers++; message = "${selected.title} is back to its last saved moment." }
    }
    // Progress is never lost: pause, background, and a low battery too, which is
    // the one that arrives without anybody touching the tablet.
    private val batteryLow = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            val selected = activeGame ?: return
            if (!surface.loaded) return
            surface.task {
                try { save(selected, -1) }
                catch (e: Exception) { runOnUiThread { report("Your progress couldn't be saved just now.", e.message) } }
            }
        }
    }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        library = Library(this)
        settings = Settings(this)
        input = ControllerInput(this) { if (game != null) pause(); message = "Controller disconnected. Your game is paused." }
        surface = GameSurface(this, input::buttons, { rewindAtStart = true }) { detail -> report("This game stopped.", detail); paused = true; busy = false }
        runCatching { games = library.games() }.onFailure { report("Your shelf couldn't be opened.", it.message) }
        ContextCompat.registerReceiver(this, batteryLow, IntentFilter(Intent.ACTION_BATTERY_LOW), ContextCompat.RECEIVER_NOT_EXPORTED)
        // Every slot a Material component actually reads is set here. The
        // defaults are purple-tinted, and any one left unset shows up as an
        // off-hue label or dialog against this green palette.
        setContent {
            MaterialTheme(
                colorScheme = darkColorScheme(
                    primary = Color(0xffb9e38c),
                    onPrimary = Color(0xff17300c),
                    background = Color(0xff111813),
                    onBackground = Color(0xffedf4e9),
                    surface = Color(0xff1d2820),
                    onSurface = Color(0xffedf4e9),
                    surfaceVariant = Color(0xff2a3a2e),
                    onSurfaceVariant = Color(0xffc3d1bd),
                    surfaceContainer = Color(0xff1d2820),
                    surfaceContainerHigh = Color(0xff243128),
                    outline = Color(0xff6d7f68),
                    error = Color(0xffffb4a6),
                    onError = Color(0xff5f1409),
                )
            ) { App() }
        }
    }
    private fun work(label: String, plain: String = "$label didn't finish.", action: () -> Unit) {
        if (busy) return
        busy = true; surface.playing = false; input.clear()
        surface.task {
            try { Native.audio(false); action() }
            catch (e: Exception) { runOnUiThread { paused = true; report(plain, e.message) } }
            finally { runOnUiThread { busy = false } }
        }
    }
    private fun openLoaded(selected: Game) {
        val battery = library.battery(selected)
        // One plain sentence covers both, because from the player's side they are
        // the same event: the game came back from further behind than expected.
        var warning: String? = null
        var detail: String? = null
        if (battery.exists()) runCatching { Native.restore(battery.readBytes(), true) }.onFailure {
            warning = "This game had to start from an earlier point."
            detail = "The battery save couldn't be opened: ${it.message}. Manual save slots are unaffected."
        }
        val auto = library.state(selected, -1)
        if (auto.exists()) runCatching { Native.restore(auto.readBytes(), false) }.onFailure {
            warning = "This game had to start from an earlier point."
            detail = "The automatic save couldn't be opened: ${it.message}. It started from the battery save; manual save slots are unaffected."
        }
        surface.setGameFrameRate(Native.frameRate())
        activeGame = selected
        surface.loaded = true
        runOnUiThread {
            game = selected; games = library.games(); slots = library.slots(selected); message = null
            warning?.let { report(it, detail) }
            if (!backgrounded && warning == null) resumeGame() else paused = true
        }
    }
    private fun open(selected: Game) = work("Opening game", "This game didn't work.") {
        surface.loaded = false
        try { Native.load(library.rom(selected)); openLoaded(selected) }
        catch (e: Exception) { runOnUiThread { game = null }; throw e }
    }
    private fun resumeGame() {
        showSlots = false; showSettings = false; paused = false
        surface.isFocusableInTouchMode = true; surface.requestFocus()
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        playingSince = SystemClock.elapsedRealtime()
        surface.task { if (!backgrounded) { Native.audio(true); surface.playing = true } }
    }
    private fun pause() {
        surface.playing = false; paused = true; input.clear()
        applyScrub(0)
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val selected = game ?: return
        if (!surface.loaded) return
        val seconds = elapsedSeconds()
        work("Saving progress", "Your progress couldn't be saved.") {
            save(selected, -1)
            library.record(selected, seconds)
            runOnUiThread { slots = library.slots(selected); games = library.games() }
        }
    }
    /** The framebuffer as a PNG. Reused by save thumbnails and by screenshots. */
    private val shot = ByteBuffer.allocateDirect(256 * 240 * 4)
    private fun framePng(): ByteArray {
        Native.frame(shot, 0, 0, false)
        val bitmap = Bitmap.createBitmap(256, 240, Bitmap.Config.ARGB_8888)
        shot.rewind(); bitmap.copyPixelsFromBuffer(shot)
        val out = ByteArrayOutputStream(); bitmap.compress(Bitmap.CompressFormat.PNG, 100, out); bitmap.recycle()
        return out.toByteArray()
    }
    private fun save(selected: Game, slot: Int) {
        val bytes = Native.snapshot(false)
        Library.atomic(library.state(selected, slot), bytes)
        val battery = Native.snapshot(true)
        if (battery.isNotEmpty()) Library.atomic(library.battery(selected), battery)
        Library.atomic(library.thumbnail(selected, slot), framePng())
    }
    /**
     * A screenshot goes to the device's own Pictures folder, so it turns up in the
     * gallery like any other picture — no permission needed for a collection this
     * app wrote itself. It runs on the GL thread to stay ordered with save and
     * rewind rather than reading the framebuffer from under them, and it does not
     * pause the game: it is a thing you do mid-play.
     *
     * The framebuffer is what lands in the file, so aspect, trim and scanlines are
     * not applied. That is the picture the console produced.
     */
    private fun screenshot() {
        val selected = game ?: return
        surface.task {
            try {
                val png = framePng()
                val stamp = SimpleDateFormat("yyyy-MM-dd HH.mm.ss", Locale.US).format(Date())
                val name = selected.title.replace(Regex("[^A-Za-z0-9 ._-]"), "").trim().ifEmpty { "Emulia" }
                val values = ContentValues().apply {
                    put(MediaStore.Images.Media.DISPLAY_NAME, "$name $stamp.png")
                    put(MediaStore.Images.Media.MIME_TYPE, "image/png")
                    put(MediaStore.Images.Media.RELATIVE_PATH, "Pictures/Emulia")
                    put(MediaStore.Images.Media.IS_PENDING, 1)
                }
                val uri = contentResolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
                    ?: error("The gallery would not accept a new picture")
                contentResolver.openOutputStream(uri)?.use { it.write(png) } ?: error("The picture could not be written")
                values.clear(); values.put(MediaStore.Images.Media.IS_PENDING, 0)
                contentResolver.update(uri, values, null, null)
                runOnUiThread { message = "Screenshot saved to Pictures/Emulia." }
            } catch (e: Exception) { runOnUiThread { report("That screenshot couldn't be saved.", e.message) } }
        }
    }
    private fun saveSlot(slot: Int) {
        val selected = game ?: return
        work("Saving slot ${slot + 1}") { save(selected, slot); runOnUiThread { slots = library.slots(selected); message = "Saved to slot ${slot + 1}." } }
    }
    private fun loadSlot(slot: Int) {
        val selected = game ?: return
        work("Loading save") { Native.restore(library.state(selected, slot).readBytes(), false); surface.requestRender(); runOnUiThread { showSlots = false; message = "Save loaded. Tap Resume when you're ready." } }
    }
    override fun onPause() {
        backgrounded = true; surface.playing = false; paused = true; input.clear()
        val seconds = elapsedSeconds()
        val done = CountDownLatch(1)
        surface.task {
            try {
                Native.audio(false)
                val selected = activeGame
                if (selected != null && surface.loaded) { save(selected, -1); library.record(selected, seconds) }
            }
            catch (e: Exception) { runOnUiThread { report("Your progress couldn't be saved.", e.message) } }
            finally { done.countDown() }
        }
        done.await(1500, TimeUnit.MILLISECONDS)
        super.onPause()
    }
    override fun onResume() { super.onResume(); backgrounded = false; games = runCatching { library.games() }.getOrDefault(games) }
    override fun onDestroy() { input.close(); unregisterReceiver(batteryLow); super.onDestroy() }
    // Capture controller buttons before focused Compose widgets consume them.
    @SuppressLint("RestrictedApi")
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        // Volume and power stay with the system; everything else is fair game
        // while mapping, including Back, because adapters do report Select as
        // Back. The wizard's own Cancel button is the way out.
        val systemKey = event.keyCode == KeyEvent.KEYCODE_VOLUME_UP ||
            event.keyCode == KeyEvent.KEYCODE_VOLUME_DOWN ||
            event.keyCode == KeyEvent.KEYCODE_VOLUME_MUTE ||
            event.keyCode == KeyEvent.KEYCODE_POWER
        if (mapping && !systemKey) {
            if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 &&
                (mappedKeys.isEmpty() || mappedKeys.first().first.deviceId == event.deviceId)) {
                if (mappedKeys.none { it.first.keyCode == event.keyCode && it.first.scanCode == event.scanCode }) {
                    mappedKeys.add(KeyEvent(event) to mapButtons[mappingStep].second)
                    mappingName = event.device?.name ?: "Keyboard"
                    mappingStep++
                    if (mappingStep == mapButtons.size) { input.saveMapping(mappedKeys); mapping = false; message = "Buttons saved for $mappingName. The directional pad and stick work automatically." }
                }
            }
            return true
        }
        // Shoulder buttons drive the time control, so a controller session never
        // has to reach for the screen to skip a cutscene or undo a fall. Bumpers
        // nudge, triggers race — the same "further means faster" as the track.
        if (game != null && !paused && !busy && event.repeatCount == 0) {
            val held = event.action == KeyEvent.ACTION_DOWN
            val speed = when (event.keyCode) {
                KeyEvent.KEYCODE_BUTTON_R1 -> 2
                KeyEvent.KEYCODE_BUTTON_R2 -> 6
                KeyEvent.KEYCODE_BUTTON_L1 -> -2
                KeyEvent.KEYCODE_BUTTON_L2 -> -6
                else -> 0
            }
            if (speed != 0) { applyScrub(if (held) speed else 0); return true }
        }
        if (game != null && !paused && input.key(event)) return true
        // Start resumes from the pause panel, so a session driven entirely from
        // the controller never has to reach for the screen. It reads through the
        // saved profile, so a remapped Start still works.
        if (game != null && paused && !busy && event.action == KeyEvent.ACTION_DOWN && input.bitFor(event) == 8) {
            resumeGame()
            return true
        }
        return super.dispatchKeyEvent(event)
    }
    override fun onGenericMotionEvent(event: MotionEvent) = if (game != null && !paused && input.motion(event)) true else super.onGenericMotionEvent(event)

    @Composable private fun App() {
        BackHandler(game != null || showSettings || showProblems) {
            when {
                showProblems -> showProblems = false
                showSettings -> closeSettings()
                fullscreen -> applyFullscreen(false)
                !paused -> pause()
                busy -> Unit
                showSlots -> showSlots = false
                else -> game = null
            }
        }
        // Presentation lives in settings and belongs to the GL context, so it is
        // pushed across whenever it changes — including once on first composition.
        LaunchedEffect(settings.aspect, settings.trimEdges, settings.filter) {
            surface.setPicture(settings.aspect, settings.trimEdges, settings.filter)
        }
        // The skip-back buttons grey out when the chain is too short to honour
        // them, so the depth has to be known while a game is open.
        LaunchedEffect(game) {
            while (game != null) { rewindDepth = surface.depth; delay(250) }
            rewindDepth = 0
        }
        // The end-of-tape notice says its piece and goes, rather than sitting
        // there until the next rewind.
        LaunchedEffect(rewindAtStart) { if (rewindAtStart) { delay(2200); rewindAtStart = false } }
        Surface(color = MaterialTheme.colorScheme.background, contentColor = MaterialTheme.colorScheme.onSurface) {
        Box(Modifier.fillMaxSize().safeDrawingPadding()) {
            // Keep the GL thread attached while the library is visible so imports
            // and state operations have a single serialized execution queue.
            Column(Modifier.fillMaxSize()) {
                if (game != null && !fullscreen) Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(game!!.title, Modifier.weight(1f), fontWeight = FontWeight.Bold, maxLines = 1)
                    BarButton(Icons.Filled.List, "Save states", !busy) { pause(); showSlots = true }
                    BarButton(ENTER_FULLSCREEN, "Full screen", !busy) { applyFullscreen(true) }
                    BarButton(Icons.Filled.Menu, "Menu", !busy) { pause() }
                }
                AndroidView(factory = { surface }, modifier = Modifier.weight(1f).fillMaxWidth())
                if (game != null && (!fullscreen || fullscreenTouch)) TouchControls()
            }
            if (game != null && fullscreen && !paused) Row(Modifier.align(Alignment.TopEnd).background(Color.Black.copy(alpha = 0.7f))) {
                BarButton(Icons.Filled.Menu, "Menu", !busy) { pause() }
                BarButton(null, if (fullscreenTouch) "Hide controls" else "Touch controls") { fullscreenTouch = !fullscreenTouch }
                BarButton(EXIT_FULLSCREEN, "Exit full screen") { applyFullscreen(false) }
            }
            // Full screen with the controls hidden still needs the time control,
            // so it gets a compact copy of the same track.
            if (game != null && fullscreen && !fullscreenTouch && !paused) Row(
                Modifier.align(Alignment.BottomCenter).padding(bottom = 18.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                SkipBack(5, 46); SkipBack(15, 46)
                TimeScrubber(Modifier.width(240.dp), 52)
            }
            if (game == null) Shelf()
            if (game != null && paused) PausePanel()
            // Drawn here, inside the activity's window, rather than as a
            // dialog: a dialog has its own window and its own key dispatch, so
            // dispatchKeyEvent never sees the button the wizard is asking for.
            if (mapping) MappingPanel()
            if (showSettings) SettingsPanel()
            // Said once when a time control runs out or is running, so a picture
            // that isn't following the buttons still explains itself.
            val notice = when {
                rewindAtStart -> "That's as far back as this goes."
                scrub != 0 -> Scrub.label(scrub)
                else -> null
            }
            notice?.let {
                Box(
                    Modifier.align(Alignment.TopCenter).padding(top = 12.dp).clip(RoundedCornerShape(16.dp))
                        .background(Color(0xff33240c)).padding(horizontal = 16.dp, vertical = 8.dp)
                ) { Text(it, color = Color(0xffffd9a0)) }
            }
            if (busy) Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.65f)), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        }
        }
        message?.let { text ->
            AlertDialog(
                onDismissRequest = { message = null },
                title = { Text("A quick update") },
                text = { Text(text) },
                confirmButton = { TextButton(onClick = { message = null }) { Text("Got it") } },
            )
        }
        // Where the real reason went. Newest first, because that is the one
        // being chased.
        if (showProblems) {
            val lines = remember(showProblems) { library.problems() }
            AlertDialog(
                onDismissRequest = { showProblems = false },
                title = { Text("Problem log") },
                text = {
                    if (lines.isEmpty()) Text("Nothing has gone wrong yet.")
                    else Column(Modifier.heightIn(max = 380.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        lines.take(40).forEach { Text(it, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    }
                },
                confirmButton = { TextButton(onClick = { showProblems = false }) { Text("Close") } },
            )
        }
        overwrite?.let { slot -> AlertDialog(onDismissRequest = { overwrite = null }, title = { Text("Replace slot ${slot + 1}?") }, text = { Text("This replaces the progress saved in this slot. Your other slots stay available.") }, confirmButton = { TextButton(onClick = { overwrite = null; saveSlot(slot) }) { Text("Replace save") } }, dismissButton = { TextButton(onClick = { overwrite = null }) { Text("Keep it") } }) }
    }
    /**
     * One control for time in both directions. Drag left of centre to run the game
     * backwards, right to run it forward, and the further from centre the faster it
     * goes; let go and the thumb springs back to the middle and play resumes. This
     * replaced a pair of hold buttons: two things that were really one axis.
     *
     * The centre is dead ([Scrub.DEAD_ZONE]) so a thumb resting slightly off
     * centre does not creep the game along.
     */
    @Composable private fun TimeScrubber(modifier: Modifier = Modifier, side: Int = 68) {
        var fraction by remember { mutableFloatStateOf(0f) }
        val speed = scrub
        val tint = when {
            speed < 0 -> Color(0xffffd9a0)
            speed > 0 -> Color(0xffc9d9ff)
            else -> Color(0xffb8c6d6)
        }
        BoxWithConstraints(
            modifier.height(side.dp).clip(RoundedCornerShape((side / 2).dp)).background(Color(0xff1b2430))
                .pointerInput(game) {
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        try {
                            var x = down.position.x
                            do {
                                val f = ((x / size.width) * 2f - 1f).coerceIn(-1f, 1f)
                                fraction = f
                                applyScrub(Scrub.speed(f))
                                val event = awaitPointerEvent()
                                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                                change.consume(); x = change.position.x
                            } while (change.pressed)
                        } finally { fraction = 0f; applyScrub(0) }
                    }
                }
                .semantics {
                    role = Role.Button
                    contentDescription = "Time control. Drag left to rewind, right to fast-forward. The further from the middle, the faster."
                    stateDescription = Scrub.label(speed)
                    customActions = listOf(
                        CustomAccessibilityAction("Back five seconds") { skipBack(5); true },
                        CustomAccessibilityAction("Back fifteen seconds") { skipBack(15); true },
                    )
                },
            contentAlignment = Alignment.Center,
        ) {
            val thumb = (side - 12).dp
            // The lambda offset overload, because the thumb moves on every touch
            // sample: this way the drag re-runs layout rather than recomposition.
            val travel = with(LocalDensity.current) { ((maxWidth - thumb) / 2).toPx() }
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("◀◀", color = Color(0xff5c6b7d), fontSize = (side * 0.19f).sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.weight(1f))
                Text("▶▶", color = Color(0xff5c6b7d), fontSize = (side * 0.19f).sp, fontWeight = FontWeight.Bold)
            }
            // The centre, so "stopped" is somewhere you can aim for.
            Box(Modifier.width(2.dp).height((side * 0.34f).dp).background(Color(0xff3d4c5e)))
            Box(
                Modifier.offset { IntOffset((travel * fraction).roundToInt(), 0) }
                    .size(thumb).clip(RoundedCornerShape(thumb / 2)).background(tint),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    if (speed == 0) "▮▮" else "${if (speed < 0) -speed else speed}×",
                    color = Color(0xff17222e),
                    fontSize = (side * (if (speed == 0) 0.2f else 0.26f)).sp,
                    fontWeight = FontWeight.Bold,
                )
            }
        }
    }
    /**
     * A known jump backwards. Dimmed and inert when the chain is shorter than the
     * jump, because offering to undo fifteen seconds that were never recorded is
     * a promise the rewind buffer cannot keep.
     */
    @Composable private fun SkipBack(seconds: Int, side: Int) {
        val ready = !busy && rewindDepth >= surface.framesFor(seconds)
        Surface(
            onClick = { skipBack(seconds) },
            enabled = ready,
            shape = RoundedCornerShape((side * 0.36f).dp),
            color = if (ready) Color(0xff35485c) else Color(0xff232c36),
            modifier = Modifier.size(side.dp).semantics { contentDescription = "Back $seconds seconds" },
        ) {
            Box(contentAlignment = Alignment.Center) {
                Text(
                    "↺$seconds",
                    color = if (ready) Color(0xffdce8f5) else Color(0xff56646f),
                    fontSize = (side * 0.28f).sp,
                    fontWeight = FontWeight.Bold,
                )
            }
        }
    }
    @Composable private fun BarButton(icon: ImageVector?, label: String, enabled: Boolean = true, onClick: () -> Unit) {
        TextButton(enabled = enabled, onClick = onClick) {
            if (icon != null) {
                // The label says the same thing, so the icon is decoration.
                Icon(icon, contentDescription = null, Modifier.size(20.dp))
                Spacer(Modifier.width(6.dp))
            }
            Text(label)
        }
    }
    @Composable private fun MappingPanel() {
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.85f)).padding(24.dp), contentAlignment = Alignment.Center) {
            Surface(shape = RoundedCornerShape(28.dp), modifier = Modifier.widthIn(max = 560.dp).fillMaxWidth()) {
                Column(Modifier.padding(28.dp), verticalArrangement = Arrangement.spacedBy(14.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("Set up your controller", fontSize = 26.sp, fontWeight = FontWeight.Bold)
                    Text(
                        if (mappingName.isEmpty()) "Use the controller you want to play with." else mappingName,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text("Press  ${mapButtons[minOf(mappingStep, mapButtons.size - 1)].first}", fontSize = 44.sp, fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.primary)
                    Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                        mapButtons.forEachIndexed { index, button ->
                            Text(
                                if (index < mappingStep) "${button.first} ✓" else button.first,
                                color = if (index < mappingStep) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                                fontWeight = if (index == mappingStep) FontWeight.Bold else FontWeight.Normal,
                            )
                        }
                    }
                    Text(
                        "Step ${minOf(mappingStep + 1, mapButtons.size)} of ${mapButtons.size}. Use a different button for each one. " +
                            "Directions come from the pad or stick, so they are not part of this. " +
                            "Shoulder buttons stay on rewind and fast-forward. " +
                            "What you choose is remembered for this controller.",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    OutlinedButton(onClick = { mapping = false; input.clear() }, modifier = Modifier.heightIn(min = 52.dp)) { Text("Cancel") }
                }
            }
        }
    }
    /**
     * One settings panel, reachable from the shelf and from a paused game, with a
     * preview of every choice that changes what the screen looks like.
     *
     * The preview is not a mock-up: it is the real shader, drawn by the real
     * renderer into an off-screen buffer and read back, so it cannot drift from
     * what the game will look like. Mid-game it previews the frame you paused on;
     * from the shelf it uses the newest saved moment, or a built pattern.
     */
    @Composable private fun SettingsPanel() {
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.85f)).padding(24.dp), contentAlignment = Alignment.Center) {
            Surface(shape = RoundedCornerShape(28.dp), modifier = Modifier.widthIn(max = 620.dp).fillMaxWidth()) {
                Column(Modifier.padding(24.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("Settings", fontSize = 28.sp, fontWeight = FontWeight.Bold)
                    SectionLabel("Picture")
                    // Redrawn whenever a choice that affects it changes.
                    LaunchedEffect(previewSample, settings.aspect, settings.trimEdges, settings.filter) { renderPreview() }
                    Box(
                        Modifier.fillMaxWidth().aspectRatio(4f / 3f).clip(RoundedCornerShape(16.dp)).background(Color.Black),
                        contentAlignment = Alignment.Center,
                    ) {
                        val shown = previewImage
                        if (shown != null) Image(shown, "Preview of the current picture settings", Modifier.fillMaxSize())
                        else Text("Preparing a preview…", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Text(
                        "The shape and look below, drawn by the same shader the game uses.",
                        Modifier.padding(top = 8.dp, bottom = 6.dp),
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    ChoiceRow("Shape", Aspect.entries.map { it.label }, settings.aspect.ordinal) { settings.aspect = Aspect.entries[it] }
                    ChoiceRow("Look", Filter.entries.map { it.label }, settings.filter.ordinal) { settings.filter = Filter.entries[it] }
                    Text(
                        settings.filter.note,
                        Modifier.padding(start = 12.dp, top = 2.dp, bottom = 6.dp),
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    SettingRow("Trim the edges", onOff(settings.trimEdges), "Hides the ${Picture.TRIM} rows a television lost to overscan") {
                        settings.trimEdges = !settings.trimEdges
                    }
                    SectionLabel("Controls")
                    SettingRow("Controller buttons", "Set up", "Map A, B, Select and Start for a controller") { closeSettings(); startMapping() }
                    SectionLabel("This device")
                    SettingRow("Problem log", "Open", "What went wrong, and why") { showProblems = true }
                    Spacer(Modifier.height(8.dp))
                    Button(onClick = { closeSettings() }, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text("Done") }
                }
            }
        }
    }
    /** A labelled row of choices. Chips rather than a cycling row, so the preview is one tap from any option. */
    @Composable private fun ChoiceRow(label: String, options: List<String>, selected: Int, onPick: (Int) -> Unit) {
        Column(Modifier.padding(vertical = 4.dp)) {
            Text(label, Modifier.padding(start = 12.dp, bottom = 6.dp), fontWeight = FontWeight.SemiBold)
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                options.forEachIndexed { index, option ->
                    FilterChip(selected = index == selected, onClick = { onPick(index) }, label = { Text(option) })
                }
            }
        }
    }
    private fun onOff(value: Boolean) = if (value) "On" else "Off"
    @Composable private fun SectionLabel(text: String) {
        Text(
            text.uppercase(),
            Modifier.padding(top = 14.dp, bottom = 2.dp),
            color = MaterialTheme.colorScheme.primary,
            fontSize = 12.sp,
            letterSpacing = 2.sp,
            fontWeight = FontWeight.Bold,
        )
    }
    /** Label on the left, current value on the right, the whole row a target. */
    @Composable private fun SettingRow(label: String, value: String, hint: String? = null, onClick: () -> Unit) {
        Surface(onClick = onClick, color = Color.Transparent, shape = RoundedCornerShape(14.dp), modifier = Modifier.fillMaxWidth()) {
            Row(Modifier.padding(horizontal = 12.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(label, fontWeight = FontWeight.SemiBold)
                    if (hint != null) Text(hint, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Text(value, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
            }
        }
    }
    @Composable private fun Shelf() {
        Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).padding(24.dp)) {
            Text("EMULIA", color = MaterialTheme.colorScheme.primary, letterSpacing = 3.sp, fontSize = 13.sp, fontWeight = FontWeight.Bold)
            Text(BuildConfig.VERSION_NAME, color = Color(0xffabb8a9), fontSize = 11.sp)
            Spacer(Modifier.height(12.dp))
            Text("Your next adventure", fontSize = 32.sp, fontWeight = FontWeight.Bold)
            Text("Pick a game. Play a little. Come back anytime.", color = Color(0xffabb8a9), modifier = Modifier.padding(top = 8.dp, bottom = 20.dp))
            Button(enabled = !busy, onClick = { importGame.launch(arrayOf("*/*")) }) { Text("＋ Add a game") }
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                TextButton(onClick = { openSettings() }) { Text("Settings") }
                TextButton(onClick = { startMapping() }) { Text("Set up controller buttons") }
            }
            if (games.isEmpty()) {
                Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text("A shelf full of possibilities", fontSize = 23.sp, fontWeight = FontWeight.SemiBold)
                        Text("Add a game file (.nes) from your tablet to begin.\nGames stay on this device. No account needed.", modifier = Modifier.padding(16.dp), color = Color(0xffabb8a9))
                    }
                }
            } else LazyVerticalGrid(columns = GridCells.Adaptive(220.dp), contentPadding = PaddingValues(top = 24.dp), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                items(games, key = { it.id }) { selected ->
                    Card(onClick = { open(selected) }, enabled = !busy, shape = RoundedCornerShape(24.dp)) {
                        Column {
                            Cover(selected.title, library.cover(selected), Modifier.fillMaxWidth().aspectRatio(4f/3f), covers)
                            Column(Modifier.padding(horizontal = 18.dp, vertical = 14.dp)) {
                                Text(selected.title, fontWeight = FontWeight.Bold, fontSize = 19.sp)
                                Text(
                                    if (library.state(selected, -1).exists()) "Resume your adventure  →" else "Ready to play  →",
                                    color = MaterialTheme.colorScheme.primary,
                                    modifier = Modifier.padding(top = 8.dp),
                                )
                                playtime(selected.seconds)?.let { Text(it, fontSize = 12.sp, color = Color(0xffabb8a9), modifier = Modifier.padding(top = 4.dp)) }
                                Row {
                                    TextButton(onClick = { artFor = selected; importArt.launch(arrayOf("image/*")) }) { Text("Box art") }
                                    if (library.art(selected).exists()) TextButton(onClick = { clearArt(selected) }) { Text("Clear") }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    @Composable private fun PausePanel() {
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.8f)).padding(24.dp), contentAlignment = Alignment.Center) {
            Surface(shape = RoundedCornerShape(28.dp), modifier = Modifier.widthIn(max = 720.dp).fillMaxWidth()) {
                Column(Modifier.padding(24.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(if (showSlots) "Save states" else "Take your time", fontSize = 28.sp, fontWeight = FontWeight.Bold)
                    Text(if (showSlots) "Ten slots, plus a separate automatic save." else "Progress saves automatically when you pause or leave.", color = Color(0xffabb8a9))
                    if (showSlots) LazyVerticalGrid(columns = GridCells.Adaptive(180.dp), modifier = Modifier.heightIn(max = 360.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        items(slots, key = { it.number }) { slot ->
                            Column(Modifier.background(Color(0xff28362b), RoundedCornerShape(16.dp)).padding(12.dp)) {
                                val image = remember(slot.time) { if (slot.thumbnail.exists()) BitmapFactory.decodeFile(slot.thumbnail.path)?.asImageBitmap() else null }
                                if (image != null) Image(image, "Save slot ${slot.number + 1}", Modifier.fillMaxWidth().aspectRatio(4f/3f))
                                Text("Slot ${slot.number + 1}", fontWeight = FontWeight.Bold)
                                Text(if (slot.time == 0L) "Empty · ready for a moment" else DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(slot.time)), fontSize = 12.sp)
                                Row {
                                    TextButton(enabled = !busy, onClick = { if (slot.time == 0L) saveSlot(slot.number) else overwrite = slot.number }) { Text("Save") }
                                    TextButton(enabled = !busy && slot.time != 0L, onClick = { loadSlot(slot.number) }) { Text("Load") }
                                }
                            }
                        }
                    }
                    Button(enabled = !busy, onClick = { resumeGame() }, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) { Text("Resume game", fontSize = 18.sp) }
                    OutlinedButton(enabled = !busy, onClick = { showSlots = !showSlots }, modifier = Modifier.fillMaxWidth()) { Text(if (showSlots) "Back to pause" else "Save states · 10 slots") }
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        TextButton(enabled = !busy, onClick = { openSettings() }) { Text("Settings") }
                        TextButton(enabled = !busy, onClick = { screenshot() }) { Text("Screenshot") }
                        TextButton(onClick = { applyFullscreen(!fullscreen) }) { Text(if (fullscreen) "Exit full screen" else "Full screen") }
                    }
                    TextButton(enabled = !busy, onClick = { applyFullscreen(false); game = null; showSlots = false }) { Text("Back to your shelf") }
                }
            }
        }
    }
    private fun pulseTouch(bit: Int) {
        if (paused) return
        input.touch = input.touch or bit
        surface.postDelayed({ input.touch = input.touch and bit.inv() }, 150)
    }
    // Control size follows the screen rather than a setting: a 13" tablet in
    // landscape has room for large targets, a narrow window does not, and asking
    // the player to decide was one question too many.
    @Composable private fun TouchControls() {
        BoxWithConstraints(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp)) {
            val compact = maxWidth < 600.dp
            val scale = (maxWidth / 900.dp).coerceIn(1f, 1.45f)
            fun size(base: Int) = (base * scale).toInt()
            Column {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                    Dpad(size(144))
                    if (!compact) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        HoldButton("Select", 4, size(64)); HoldButton("Start", 8, size(64))
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) { HoldButton("B", 2, size(68)); HoldButton("A", 1, size(76)) }
                }
                // Time sits on its own row under the pad, because it is a wide
                // gesture and it belongs to neither hand in particular.
                Row(
                    Modifier.fillMaxWidth().padding(top = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    SkipBack(5, size(56)); SkipBack(15, size(56))
                    TimeScrubber(Modifier.weight(1f), size(60))
                    if (compact) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        HoldButton("Select", 4, size(52)); HoldButton("Start", 8, size(52))
                    }
                }
            }
        }
    }
    @Composable private fun Dpad(side: Int) {
            Box(Modifier.size(side.dp).semantics {
                contentDescription = "Directional pad. Slide to move."
                customActions = listOf("Up" to 16, "Down" to 32, "Left" to 64, "Right" to 128).map { (label, bit) -> CustomAccessibilityAction(label) { pulseTouch(bit); true } }
            }.clip(RoundedCornerShape(32.dp)).background(Color(0xff28362b)).pointerInput(paused) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    try {
                        var point = down.position
                        do {
                            val x = point.x / size.width; val y = point.y / size.height
                            val bits = (if (x < .35f) 64 else if (x > .65f) 128 else 0) or (if (y < .35f) 16 else if (y > .65f) 32 else 0)
                            if (!paused) input.touch = (input.touch and 15) or bits
                            val event = awaitPointerEvent(); val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            change.consume(); point = change.position
                        } while (change.pressed)
                    } finally { input.touch = input.touch and 0xf0.inv() }
                }
            }, contentAlignment = Alignment.Center) { Text("✚", fontSize = (side * 0.51f).sp, color = Color(0xffb6c9af)) }
    }
    @Composable private fun HoldButton(label: String, bit: Int, size: Int) {
        var held by remember { mutableStateOf(false) }
        Box(Modifier.size(size.dp).semantics {
            role = Role.Button; contentDescription = "$label button"
            stateDescription = if (held) "Pressed" else "Released"
            onClick { pulseTouch(bit); true }
        }.clip(RoundedCornerShape(28.dp)).background(if (held) Color(0xffd2f9ac) else Color(0xffb9e38c)).pointerInput(paused) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                try {
                    if (!paused) { input.touch = input.touch or bit; held = true }
                    do { val event = awaitPointerEvent(); val change = event.changes.firstOrNull { it.id == down.id } ?: break; change.consume() } while (change.pressed)
                } finally { input.touch = input.touch and bit.inv(); held = false }
            }
        }, contentAlignment = Alignment.Center) { Text(label, color = Color(0xff203018), fontSize = (size * if (label.length == 1) 0.37f else 0.2f).sp, fontWeight = FontWeight.Bold) }
    }
}

/** Playtime worth showing. Under a minute is noise on a shelf. */
private fun playtime(seconds: Long): String? = when {
    seconds < 60 -> null
    seconds < 3600 -> "${seconds / 60} min played"
    else -> "${seconds / 3600} h ${(seconds % 3600) / 60} min played"
}

/** Box art is decoded to fill a tile a few hundred dp wide; this is ample. */
private const val ART_MAX_EDGE = 1024f

// The settings preview buffer. 4:3, and tall enough that pixel-perfect reaches a
// second whole multiple rather than showing a postage stamp in a wide border.
private const val PREVIEW_WIDTH = 640
private const val PREVIEW_HEIGHT = 480

private fun cornerIcon(name: String, path: String): ImageVector = ImageVector.Builder(
    name = name,
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).addPath(PathParser().parsePathString(path).toNodes(), fill = SolidColor(Color.White)).build()

private val ENTER_FULLSCREEN = cornerIcon(
    "EnterFullscreen",
    "M7,14H5v5h5v-2H7V14zM5,10h2V7h3V5H5V10zM17,17h-3v2h5v-5h-2V17zM14,5v2h3v3h2V5H14z",
)
private val EXIT_FULLSCREEN = cornerIcon(
    "ExitFullscreen",
    "M5,16h3v3h2v-5H5V16zM8,8H5v2h5V5H8V8zM14,19h2v-3h3v-2h-5V19zM16,8V5h-2v5h5V8H16z",
)
