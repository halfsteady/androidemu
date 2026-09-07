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
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.PointerEventPass
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
    /** The shelf can show what has been put away, so it can be brought back. */
    private var showArchive by mutableStateOf(false)
    private var archived by mutableStateOf(emptyList<Game>())
    private var forgetting by mutableStateOf<Game?>(null)
    private fun refreshLibrary() {
        games = runCatching { library.games() }.getOrDefault(games)
        archived = runCatching { library.archived() }.getOrDefault(archived)
    }
    /** Off the shelf, nothing on disk touched. Reversible, so no confirmation. */
    private fun archive(selected: Game) {
        library.setArchived(selected, true)
        refreshLibrary()
        message = "${selected.title} is put away. Tap Put away on the shelf to bring it back."
    }
    private fun unarchive(selected: Game) {
        library.setArchived(selected, false)
        refreshLibrary()
        if (library.archived().isEmpty()) showArchive = false
        message = "${selected.title} is back on the shelf, exactly where you left it."
    }
    private fun forget(selected: Game) {
        library.forget(selected)
        refreshLibrary()
        if (archived.isEmpty()) showArchive = false
        message = "${selected.title} and its saves are gone from this device."
    }
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
    /**
     * Whether the full-screen chrome is on show. A tap on the picture toggles it,
     * and it stands down by itself after [OVERLAY_IDLE] of nothing happening, so
     * a game being played never keeps a menu bar over it.
     *
     * This governs the things drawn *over* the picture. The touch controls are a
     * deliberate choice that takes layout space, so they are not on this timer:
     * resizing the play area every five seconds would be worse than a bar.
     */
    private var overlays by mutableStateOf(true)
    // Not Compose state on purpose: every pointer event touches this, and waking
    // a recomposition per touch move to run a timer would be silly.
    @Volatile private var lastTouch = 0L
    private fun touched() { lastTouch = SystemClock.uptimeMillis() }
    private fun showOverlays() { overlays = true; touched() }
    private var mapping by mutableStateOf(false)
    private var mappingStep by mutableStateOf(0)
    private var mappingName by mutableStateOf("")
    private val mappedKeys = mutableListOf<Pair<KeyEvent, Int>>()
    private val mapButtons = listOf("A" to 1, "B" to 2, "Select" to 4, "Start" to 8)
    private fun applyFullscreen(value: Boolean) {
        fullscreen = value
        showOverlays()
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
            // A saved screenshot is finished colour and cannot be restained, so
            // when a palette is being chosen the built pattern is the honest
            // thing to show.
            previewSample =
                if (settings.palette == Palette.Standard) savedMoment() ?: framebufferOf(SampleFrame.pixels(settings.paletteColours()))
                else framebufferOf(SampleFrame.pixels(settings.paletteColours()))
        }
    }
    /** Lets go of the preview buffers; the next open captures a fresh sample. */
    private fun closeSettings() { showSettings = false; previewImage = null; previewSample = null }

    /**
     * Hands the core the chosen colours and repaints whatever is on screen.
     *
     * A palette is a lookup the core was doing anyway, so this costs nothing per
     * frame — but a paused game has already painted its last frame, and a preview
     * built from a captured buffer holds colour rather than indices. Both have to
     * be asked for again, or the choice appears to do nothing until play resumes.
     */
    private fun applyPalette() {
        val bytes = settings.paletteBytes()
        surface.task { Native.setPalette(bytes) }
        if (game != null && surface.loaded) {
            surface.task {
                Native.repaint(shot)
                val copy = ByteBuffer.allocateDirect(shot.capacity()).order(ByteOrder.nativeOrder())
                shot.position(0); copy.put(shot); copy.position(0); shot.position(0)
                runOnUiThread { previewSample = copy }
            }
            surface.requestRender()
        } else if (showSettings) {
            previewSample = framebufferOf(SampleFrame.pixels(settings.paletteColours()))
        }
    }
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
    /**
     * A `.pal` file: 192 bytes of RGB, the format every published NES palette
     * comes in. Longer files carry the emphasis variants and are accepted, since
     * the first 64 colours are the part the core can use today.
     */
    private val importPalette = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@registerForActivityResult
        try {
            val bytes = library.readBytes(uri)
            PaletteModel.parse(bytes) ?: error("A .pal file is at least 192 bytes; this one is ${bytes.size}")
            settings.importedPalette = bytes
            settings.palette = Palette.File
            applyPalette()
            message = "Palette loaded."
        } catch (e: Exception) {
            report("That palette file didn't work.", e.message)
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
        runCatching { refreshLibrary() }.onFailure { report("Your shelf couldn't be opened.", it.message) }
        ContextCompat.registerReceiver(this, batteryLow, IntentFilter(Intent.ACTION_BATTERY_LOW), ContextCompat.RECEIVER_NOT_EXPORTED)
        // The scheme lives in Ui, with the rest of the colour.
        setContent { MaterialTheme(colorScheme = Ui.scheme) { App() } }
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
        // A header that had to be corrected is usually the reason a game looks
        // wrong, so it is recorded rather than fixed silently. Not shown as a
        // dialog: the game plays, and this is for whoever goes looking later.
        runCatching { Native.headerNotes() }.getOrNull()?.forEach {
            library.logProblem("Header corrected for ${selected.title}.", it)
        }
        surface.setGameFrameRate(Native.frameRate())
        activeGame = selected
        surface.loaded = true
        runOnUiThread {
            game = selected; refreshLibrary(); slots = library.slots(selected); message = null
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
            runOnUiThread { slots = library.slots(selected); refreshLibrary() }
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
    override fun onResume() { super.onResume(); backgrounded = false; refreshLibrary() }
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
        BackHandler(game != null || showSettings || showProblems || showArchive) {
            when {
                showProblems -> showProblems = false
                showSettings -> closeSettings()
                showArchive -> showArchive = false
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
        // Colour is pushed to the core rather than to the GL context: the core
        // paints indices into RGB and this is the table it reads. Once on first
        // composition, so a chosen palette survives being closed and reopened.
        LaunchedEffect(settings.palette, settings.importedPalette) { applyPalette() }
        // The skip-back buttons grey out when the chain is too short to honour
        // them, so the depth has to be known while a game is open.
        LaunchedEffect(game) {
            while (game != null) { rewindDepth = surface.depth; delay(250) }
            rewindDepth = 0
        }
        // The end-of-tape notice says its piece and goes, rather than sitting
        // there until the next rewind.
        LaunchedEffect(rewindAtStart) { if (rewindAtStart) { delay(2200); rewindAtStart = false } }
        // The idle clock. Observed at the Initial pass and never consumed, so
        // every gesture below still behaves exactly as it did.
        val watchTouches = Modifier.pointerInput(Unit) {
            awaitPointerEventScope {
                while (true) { awaitPointerEvent(PointerEventPass.Initial); touched() }
            }
        }
        // One ticker while the chrome could be up, rather than a coroutine
        // restarted on every touch move.
        LaunchedEffect(fullscreen, paused, game) {
            while (fullscreen && !paused && game != null) {
                delay(500)
                if (overlays && SystemClock.uptimeMillis() - lastTouch > OVERLAY_IDLE) overlays = false
            }
        }
        Surface(color = MaterialTheme.colorScheme.background, contentColor = MaterialTheme.colorScheme.onSurface) {
        Box(Modifier.fillMaxSize().safeDrawingPadding().then(watchTouches)) {
            // Keep the GL thread attached while the library is visible so imports
            // and state operations have a single serialized execution queue.
            Column(Modifier.fillMaxSize()) {
                // Two buttons, not four. Save states used to sit here as well as
                // in the menu, and reaching it always paused the game first —
                // which is what Menu does, so it was the same two taps wearing
                // one extra button.
                if (game != null && !fullscreen) Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(game!!.title, Modifier.weight(1f), fontWeight = FontWeight.Bold, maxLines = 1)
                    BarButton(ENTER_FULLSCREEN, "Full screen", !busy) { applyFullscreen(true) }
                    BarButton(MENU, "Menu", !busy) { pause() }
                }
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    AndroidView(factory = { surface }, modifier = Modifier.fillMaxSize())
                    // In full screen the picture is the control: a tap brings the
                    // chrome back, and a second tap sends it away again. Only a
                    // tap, so a stray finger during play does nothing.
                    if (fullscreen && !paused) Box(
                        Modifier.fillMaxSize().pointerInput(Unit) {
                            detectTapGestures { if (overlays) overlays = false else showOverlays() }
                        }
                    )
                }
                if (game != null && (!fullscreen || fullscreenTouch)) TouchControls()
            }
            // A rounded pill on the app's own scrim, so the chrome over the
            // picture is recognisably the same bar as the one above it rather
            // than a black rectangle stuck to the corner.
            if (game != null && fullscreen && !paused && overlays) Row(
                Modifier.align(Alignment.TopEnd).padding(12.dp)
                    .clip(RoundedCornerShape(Ui.cornerMedium)).background(Ui.chrome).padding(horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BarButton(MENU, "Menu", !busy) { pause() }
                BarButton(GAMEPAD, if (fullscreenTouch) "Hide controls" else "Touch controls") { fullscreenTouch = !fullscreenTouch }
                BarButton(EXIT_FULLSCREEN, "Exit full screen") { applyFullscreen(false) }
            }
            // Full screen with the controls hidden still needs the time control,
            // so it gets a compact copy of the same track.
            if (game != null && fullscreen && !fullscreenTouch && !paused && overlays) Row(
                Modifier.align(Alignment.BottomCenter).padding(bottom = 18.dp)
                    .clip(RoundedCornerShape(Ui.cornerLarge)).background(Ui.chrome).padding(10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                SkipBack(5, 46); SkipBack(15, 46)
                TimeScrubber(Modifier.width(240.dp), 52)
            }
            if (game == null) Shelf()
            // One panel, one job: the pause menu offers places to go, and save
            // states is one of them rather than the same panel with a different
            // title and a button that toggles between the two.
            if (game != null && paused) if (showSlots) SlotsPanel() else PausePanel()
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
                    Modifier.align(Alignment.TopCenter).padding(top = 12.dp).clip(RoundedCornerShape(Ui.cornerSmall))
                        .background(Ui.noticeBack).padding(horizontal = 16.dp, vertical = 10.dp)
                ) { Text(it, color = Ui.noticeText, fontWeight = FontWeight.SemiBold) }
            }
            if (busy) Box(Modifier.fillMaxSize().background(Ui.scrim), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
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
        forgetting?.let { selected ->
            AlertDialog(
                onDismissRequest = { forgetting = null },
                title = { Text("Delete ${selected.title}?") },
                text = {
                    Text(
                        "This removes the game, its battery save and all ten of its save states " +
                            "from this device. It cannot be undone.\n\n" +
                            "To take it off the shelf without losing anything, leave it put away instead."
                    )
                },
                confirmButton = {
                    TextButton(onClick = { val doomed = selected; forgetting = null; forget(doomed) }) {
                        Text("Delete forever", color = MaterialTheme.colorScheme.error)
                    }
                },
                dismissButton = { TextButton(onClick = { forgetting = null }) { Text("Keep it") } },
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
     * centre does not creep the game along. At rest the handle shows a pause
     * glyph and tapping it pauses — it read as a pause button before it was one,
     * which is the kind of lie an interface should not tell.
     */
    @Composable private fun TimeScrubber(modifier: Modifier = Modifier, side: Int = 68) {
        var fraction by remember { mutableFloatStateOf(0f) }
        val speed = scrub
        val tint = when {
            speed < 0 -> Ui.backward
            speed > 0 -> Ui.forward
            else -> Ui.keyGlyph
        }
        BoxWithConstraints(
            modifier.height(side.dp).clip(RoundedCornerShape((side / 2).dp)).background(Ui.well)
                .pointerInput(game) {
                    // The handle carries a pause glyph, so it pauses: a press on
                    // it that never turns into a drag is a tap on that button.
                    // Anywhere else on the track is a scrub from the first touch,
                    // which is why the handle is the only part that can pause.
                    val handle = (side - 12).dp.toPx() / 2f
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        val onHandle = kotlin.math.abs(down.position.x - size.width / 2f) <= handle
                        var dragged = false
                        try {
                            var x = down.position.x
                            do {
                                if (kotlin.math.abs(x - down.position.x) > viewConfiguration.touchSlop) dragged = true
                                if (dragged || !onHandle) {
                                    val f = ((x / size.width) * 2f - 1f).coerceIn(-1f, 1f)
                                    fraction = f
                                    applyScrub(Scrub.speed(f))
                                }
                                val event = awaitPointerEvent()
                                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                                change.consume(); x = change.position.x
                            } while (change.pressed)
                        } finally {
                            fraction = 0f
                            applyScrub(0)
                            if (onHandle && !dragged) pause()
                        }
                    }
                }
                .semantics {
                    role = Role.Button
                    contentDescription = "Time control. Drag left to rewind, right to fast-forward — the further from the middle, the faster. Tap the handle to pause."
                    stateDescription = Scrub.label(speed)
                    customActions = listOf(
                        CustomAccessibilityAction("Pause") { pause(); true },
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
                Text("◀◀", color = Ui.wellMark, fontSize = (side * 0.19f).sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.weight(1f))
                Text("▶▶", color = Ui.wellMark, fontSize = (side * 0.19f).sp, fontWeight = FontWeight.Bold)
            }
            // The centre, so "stopped" is somewhere you can aim for.
            Box(Modifier.width(2.dp).height((side * 0.34f).dp).background(Ui.wellEdge))
            Box(
                Modifier.offset { IntOffset((travel * fraction).roundToInt(), 0) }
                    .size(thumb).clip(RoundedCornerShape(thumb / 2)).background(tint),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    if (speed == 0) "▮▮" else "${if (speed < 0) -speed else speed}×",
                    color = Ui.onTime,
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
            color = if (ready) Ui.keyFace else Ui.keyDim,
            modifier = Modifier.size(side.dp).semantics { contentDescription = "Back $seconds seconds" },
        ) {
            Box(contentAlignment = Alignment.Center) {
                Text(
                    "↺$seconds",
                    color = if (ready) Ui.keyGlyph else Ui.keyDimGlyph,
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
        Panel(
            "Set up your controller",
            if (mappingName.isEmpty()) "Use the controller you want to play with." else mappingName,
            maxWidth = 560.dp,
        ) {
            Column(
                Modifier.fillMaxWidth().padding(vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    "Press  ${mapButtons[minOf(mappingStep, mapButtons.size - 1)].first}",
                    fontSize = 44.sp,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.primary,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    mapButtons.forEachIndexed { index, button ->
                        Text(
                            if (index < mappingStep) "${button.first} ✓" else button.first,
                            color = if (index < mappingStep) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                            fontWeight = if (index == mappingStep) FontWeight.Bold else FontWeight.Normal,
                        )
                    }
                }
            }
            Text(
                "Step ${minOf(mappingStep + 1, mapButtons.size)} of ${mapButtons.size}. Use a different button for each one. " +
                    "Directions come from the pad or stick, so they are not part of this. " +
                    "Shoulder buttons stay on rewind and fast-forward. " +
                    "What you choose is remembered for this controller.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            SecondaryAction("Cancel", Modifier.fillMaxWidth()) { mapping = false; input.clear() }
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
        Panel("Settings") {
            SectionLabel("Picture")
            // Redrawn whenever a choice that affects it changes.
            LaunchedEffect(previewSample, settings.aspect, settings.trimEdges, settings.filter, settings.palette) { renderPreview() }
            Box(
                Modifier.fillMaxWidth().aspectRatio(4f / 3f).clip(RoundedCornerShape(Ui.cornerMedium)).background(Color.Black),
                contentAlignment = Alignment.Center,
            ) {
                val shown = previewImage
                if (shown != null) Image(shown, "Preview of the current picture settings", Modifier.fillMaxSize())
                else Text("Preparing a preview…", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            ChoiceNote("The shape and look below, drawn by the same shader the game uses.")
            ChoiceRow("Shape", Aspect.entries.map { it.label }, settings.aspect.ordinal) { settings.aspect = Aspect.entries[it] }
            ChoiceRow("Look", Filter.entries.map { it.label }, Filter.entries.indexOf(settings.filter)) { settings.filter = Filter.entries[it] }
            ChoiceNote(settings.filter.note)
            // Colour is its own choice, not part of the look: you might want
            // Composite with one set of colours and Cartoon with another, and
            // folding them together would multiply the list.
            ChoiceRow("Colours", Palette.entries.map { it.label }, Palette.entries.indexOf(settings.palette)) { chosen ->
                val palette = Palette.entries[chosen]
                if (palette == Palette.File && settings.importedPalette == null) importPalette.launch(arrayOf("*/*"))
                else { settings.palette = palette; applyPalette() }
            }
            ChoiceNote(settings.palette.note)
            if (settings.palette == Palette.File) {
                ValueRow("Palette file", "Replace", "Load a different .pal file") { importPalette.launch(arrayOf("*/*")) }
            }
            // A switch, because it is one. It used to be a value row reading
            // "On" or "Off", which looks like something with more than two
            // answers hiding behind it.
            SwitchRow("Trim the edges", settings.trimEdges, "Hides the ${Picture.TRIM} rows a television lost to overscan") {
                settings.trimEdges = !settings.trimEdges
            }
            SectionLabel("Controls")
            ValueRow("Controller buttons", "Set up", "Map A, B, Select and Start for a controller") { closeSettings(); startMapping() }
            SectionLabel("This device")
            // Polled only while the panel is open. The plan asks for a
            // measurable audio figure rather than a claim, so it is on
            // screen where it can be read off the tablet.
            var audio by remember { mutableStateOf(FloatArray(5)) }
            LaunchedEffect(showSettings) {
                while (showSettings) {
                    audio = runCatching { Native.audioStats() }.getOrDefault(FloatArray(5))
                    delay(500)
                }
            }
            InfoRow(
                "Audio delay",
                if (audio[2] <= 0f) "—" else "%.1f ms".format(audio[2]),
                "%.1f ms queued + %.1f ms in the device, holding %.1f ms · %d underruns"
                    .format(audio[0], audio[1], audio[3], audio[4].toInt()),
            )
            InfoRow("Version", BuildConfig.VERSION_NAME, "Emulia, on this device")
            ValueRow("Problem log", "Open", "What went wrong, and why") { showProblems = true }
            Spacer(Modifier.height(8.dp))
            PrimaryAction("Done", Modifier.fillMaxWidth()) { closeSettings() }
        }
    }
    /**
     * The shelf. One header row rather than a stacked masthead, so the games
     * start near the top of the screen instead of below three paragraphs — the
     * grid is the point of this screen and it should look like it.
     *
     * The version moved into Settings. It is a thing you look up once, not a
     * thing you read every time you sit down to play.
     */
    @Composable private fun Shelf() {
        Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).padding(horizontal = 24.dp, vertical = 18.dp)) {
            // Beside the heading where there is room for it, underneath where
            // there is not. The tablet this is for is always the first case; a
            // narrow window should still not push a button off the edge.
            BoxWithConstraints(Modifier.fillMaxWidth()) {
                if (maxWidth >= 820.dp) Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    ShelfHeading(Modifier.weight(1f))
                    ShelfActions()
                } else Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    ShelfHeading()
                    Row(
                        Modifier.horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) { ShelfActions() }
                }
            }
            Text(
                if (showArchive) "Off the shelf, and nothing lost. Every save is still here."
                else "Pick a game. Play a little. Come back anytime.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 8.dp),
            )
            val shown = if (showArchive) archived else games
            if (shown.isEmpty()) {
                Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text("A shelf full of possibilities", fontSize = 23.sp, fontWeight = FontWeight.SemiBold)
                        Text(
                            "Add a game file (.nes) from your tablet to begin.\nGames stay on this device. No account needed.",
                            modifier = Modifier.padding(16.dp),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            } else LazyVerticalGrid(
                columns = GridCells.Adaptive(220.dp),
                contentPadding = PaddingValues(top = 20.dp, bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                items(shown, key = { it.id }) { selected -> GameCard(selected) }
            }
        }
    }
    @Composable private fun ShelfHeading(modifier: Modifier = Modifier) {
        Column(modifier) {
            Text("EMULIA", color = MaterialTheme.colorScheme.primary, letterSpacing = 3.sp, fontSize = 12.sp, fontWeight = FontWeight.Bold)
            Text(if (showArchive) "Put away" else "Your next adventure", fontSize = 30.sp, fontWeight = FontWeight.Bold)
        }
    }
    @Composable private fun ShelfActions() {
        if (showArchive) SecondaryAction("Back to the shelf", icon = BACK) { showArchive = false }
        else {
            PrimaryAction("Add a game", icon = ADD, enabled = !busy, height = Ui.secondaryHeight) {
                importGame.launch(arrayOf("*/*"))
            }
            // Controller setup used to sit here as well as in Settings. One home
            // each: this is a shelf, and that is a setting.
            SecondaryAction("Settings", icon = TUNE) { openSettings() }
            // Only offered when there is something in it, so an empty shelf does
            // not advertise an empty cupboard.
            if (archived.isNotEmpty()) SecondaryAction("Put away (${archived.size})") { showArchive = true }
        }
    }
    /**
     * A game on the shelf. The whole card is one thing to tap, and what it does
     * is play — which is the only reason to be on this screen.
     *
     * Box art and putting a game away used to be three text buttons on the face
     * of every card, one of which took the game off the shelf from directly
     * under the finger aiming to start it. They live behind the card's own menu
     * now: still one tap away, no longer in the way of the game.
     */
    @Composable private fun GameCard(selected: Game) {
        var menu by remember { mutableStateOf(false) }
        // A put-away game does not open on a tap: the whole card would
        // otherwise be a trap next to "Bring back".
        Card(
            onClick = { if (!showArchive) open(selected) },
            enabled = !busy && !showArchive,
            shape = RoundedCornerShape(Ui.cornerLarge),
        ) {
            Column {
                Box {
                    Cover(selected.title, library.cover(selected), Modifier.fillMaxWidth().aspectRatio(4f / 3f), covers)
                    if (!showArchive) Box(Modifier.align(Alignment.TopEnd).padding(8.dp)) {
                        Surface(color = Ui.chrome, shape = CircleShape, contentColor = Ui.keyGlyph) {
                            IconButton(onClick = { menu = true }, enabled = !busy) {
                                Icon(MORE, "More for ${selected.title}", Modifier.size(22.dp))
                            }
                        }
                        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                            DropdownMenuItem(
                                text = { Text("Choose box art") },
                                onClick = { menu = false; artFor = selected; importArt.launch(arrayOf("image/*")) },
                            )
                            if (library.art(selected).exists()) DropdownMenuItem(
                                text = { Text("Clear box art") },
                                onClick = { menu = false; clearArt(selected) },
                            )
                            DropdownMenuItem(
                                text = { Text("Put this away") },
                                enabled = !busy,
                                onClick = { menu = false; archive(selected) },
                            )
                        }
                    }
                }
                Column(Modifier.padding(horizontal = 18.dp, vertical = 14.dp)) {
                    Text(selected.title, fontWeight = FontWeight.Bold, fontSize = 19.sp)
                    if (!showArchive) Text(
                        if (library.state(selected, -1).exists()) "Resume your adventure  →" else "Ready to play  →",
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                    playtime(selected.seconds)?.let {
                        Text(it, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 4.dp))
                    }
                    if (showArchive) Row(Modifier.padding(top = 4.dp)) {
                        QuietAction("Bring back", enabled = !busy) { unarchive(selected) }
                        QuietAction("Delete", enabled = !busy, danger = true) { forgetting = selected }
                    }
                }
            }
        }
    }
    /**
     * The pause menu: resume, or go somewhere. Everything on the way out used to
     * be a row of text buttons of equal weight, in which "Back to your shelf"
     * looked exactly like "Screenshot". Tiles of one size, and the way back
     * drawn as the different kind of thing it is.
     */
    @Composable private fun PausePanel() {
        Panel("Take your time", "Progress saves automatically when you pause or leave.", maxWidth = 560.dp) {
            PrimaryAction("Resume game", Modifier.fillMaxWidth(), PLAY, !busy) { resumeGame() }
            Spacer(Modifier.height(2.dp))
            // Intrinsic height so a label that wraps in a narrow window takes
            // its neighbour with it, rather than leaving two tiles of different
            // sizes side by side.
            Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                ActionTile(SLOTS, "Save states", Modifier.weight(1f).fillMaxHeight(), !busy) { showSlots = true }
                ActionTile(CAMERA, "Screenshot", Modifier.weight(1f).fillMaxHeight(), !busy) { screenshot() }
            }
            Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                ActionTile(
                    if (fullscreen) EXIT_FULLSCREEN else ENTER_FULLSCREEN,
                    if (fullscreen) "Exit full screen" else "Full screen",
                    Modifier.weight(1f).fillMaxHeight(),
                ) { applyFullscreen(!fullscreen) }
                ActionTile(TUNE, "Settings", Modifier.weight(1f).fillMaxHeight(), !busy) { openSettings() }
            }
            Spacer(Modifier.height(2.dp))
            SecondaryAction("Back to your shelf", Modifier.fillMaxWidth(), SHELF, !busy) {
                applyFullscreen(false); game = null; showSlots = false
            }
        }
    }
    /** Save states, as its own panel with its own way back. */
    @Composable private fun SlotsPanel() {
        Panel("Save states", "Ten slots, plus a separate automatic save.", onBack = { showSlots = false }, maxWidth = 720.dp) {
            LazyVerticalGrid(
                columns = GridCells.Adaptive(180.dp),
                modifier = Modifier.heightIn(max = 380.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                items(slots, key = { it.number }) { slot ->
                    Column(Modifier.background(Ui.keyFace, RoundedCornerShape(Ui.cornerMedium)).padding(12.dp)) {
                        val image = remember(slot.time) { if (slot.thumbnail.exists()) BitmapFactory.decodeFile(slot.thumbnail.path)?.asImageBitmap() else null }
                        if (image != null) Image(
                            image,
                            "Save slot ${slot.number + 1}",
                            Modifier.fillMaxWidth().aspectRatio(4f / 3f).clip(RoundedCornerShape(Ui.cornerSmall)),
                        )
                        Text("Slot ${slot.number + 1}", Modifier.padding(top = 6.dp), fontWeight = FontWeight.Bold)
                        Text(
                            if (slot.time == 0L) "Empty · ready for a moment"
                            else DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(slot.time)),
                            fontSize = 12.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            QuietAction("Save", enabled = !busy, compact = true) {
                                if (slot.time == 0L) saveSlot(slot.number) else overwrite = slot.number
                            }
                            QuietAction("Load", enabled = !busy && slot.time != 0L, compact = true) { loadSlot(slot.number) }
                        }
                    }
                }
            }
            Spacer(Modifier.height(2.dp))
            PrimaryAction("Resume game", Modifier.fillMaxWidth(), PLAY, !busy) { resumeGame() }
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
            }.clip(RoundedCornerShape(Ui.cornerLarge)).background(Ui.keyFace).pointerInput(paused) {
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
            }, contentAlignment = Alignment.Center) { Text("✚", fontSize = (side * 0.51f).sp, color = Ui.keyGlyph) }
    }
    @Composable private fun HoldButton(label: String, bit: Int, size: Int) {
        var held by remember { mutableStateOf(false) }
        Box(Modifier.size(size.dp).semantics {
            role = Role.Button; contentDescription = "$label button"
            stateDescription = if (held) "Pressed" else "Released"
            onClick { pulseTouch(bit); true }
        }.clip(RoundedCornerShape(Ui.cornerLarge)).background(if (held) Ui.buttonHeld else Ui.button).pointerInput(paused) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                try {
                    if (!paused) { input.touch = input.touch or bit; held = true }
                    do { val event = awaitPointerEvent(); val change = event.changes.firstOrNull { it.id == down.id } ?: break; change.consume() } while (change.pressed)
                } finally { input.touch = input.touch and bit.inv(); held = false }
            }
        }, contentAlignment = Alignment.Center) { Text(label, color = Ui.onButton, fontSize = (size * if (label.length == 1) 0.37f else 0.2f).sp, fontWeight = FontWeight.Bold) }
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

/** How long the full-screen chrome waits before standing down. */
private const val OVERLAY_IDLE = 5_000L

// The settings preview buffer. 4:3, and tall enough that pixel-perfect reaches a
// second whole multiple rather than showing a postage stamp in a wide border.
private const val PREVIEW_WIDTH = 640
private const val PREVIEW_HEIGHT = 480
