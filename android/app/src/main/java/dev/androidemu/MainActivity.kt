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
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.text.DateFormat
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

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
    private var rewinding by mutableStateOf(false)
    private var rewindAtStart by mutableStateOf(false)
    private var forwarding by mutableStateOf(false)
    // Rewinding plays no new audio, so the stream is stopped rather than left
    // to drain into an underrun, and started again when normal play resumes.
    private fun applyRewinding(active: Boolean) {
        if (rewinding == active || game == null || (active && forwarding)) return
        rewinding = active
        if (active) rewindAtStart = false
        surface.rewinding = active
        input.clear()
        surface.task { Native.audio(!active && surface.playing && !backgrounded) }
    }
    /**
     * Fast-forward runs several emulated frames per displayed frame for as long as
     * the button is held. Audio stops rather than playing back at four times the
     * pitch, and the held input carries through — unlike rewind, which clears it,
     * because holding right through a fast-forward is exactly the point.
     */
    private fun applyForwarding(active: Boolean) {
        if (forwarding == active || game == null || (active && rewinding)) return
        forwarding = active
        surface.speed = if (active) settings.fastForward else 1
        surface.task { Native.audio(!active && surface.playing && !backgrounded) }
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
        applyForwarding(false)
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
        // Shoulder buttons drive the two time controls, so a controller session
        // never has to reach for the screen to skip a cutscene or undo a fall.
        if (game != null && !paused && !busy && event.repeatCount == 0) {
            val held = event.action == KeyEvent.ACTION_DOWN
            when (event.keyCode) {
                KeyEvent.KEYCODE_BUTTON_R1, KeyEvent.KEYCODE_BUTTON_R2 -> { applyForwarding(held); return true }
                KeyEvent.KEYCODE_BUTTON_L1, KeyEvent.KEYCODE_BUTTON_L2 -> { applyRewinding(held); return true }
            }
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
                showSettings -> showSettings = false
                fullscreen -> applyFullscreen(false)
                !paused -> pause()
                busy -> Unit
                showSlots -> showSlots = false
                else -> game = null
            }
        }
        // Presentation lives in settings and belongs to the GL context, so it is
        // pushed across whenever it changes — including once on first composition.
        LaunchedEffect(settings.aspect, settings.trimEdges, settings.scanlines) {
            surface.setPicture(settings.aspect, settings.trimEdges, settings.scanlines)
        }
        Surface(color = MaterialTheme.colorScheme.background, contentColor = MaterialTheme.colorScheme.onSurface) {
        Box(Modifier.fillMaxSize().safeDrawingPadding()) {
            // Keep the GL thread attached while the library is visible so imports
            // and state operations have a single serialized execution queue.
            Column(Modifier.fillMaxSize()) {
                if (game != null && !fullscreen) Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(game!!.title, Modifier.weight(1f), fontWeight = FontWeight.Bold, maxLines = 1)
                    BarButton(Icons.Filled.List, "Save states", !busy) { pause(); showSlots = true }
                    BarButton(ENTER_FULLSCREEN, "Full screen", !busy) { applyFullscreen(true) }
                    RewindBarButton()
                    ForwardBarButton()
                    BarButton(Icons.Filled.Menu, "Menu", !busy) { pause() }
                }
                AndroidView(factory = { surface }, modifier = Modifier.weight(1f).fillMaxWidth())
                if (game != null && (!fullscreen || fullscreenTouch)) TouchControls()
            }
            if (game != null && fullscreen && !paused) Row(Modifier.align(Alignment.TopEnd).background(Color.Black.copy(alpha = 0.7f))) {
                BarButton(Icons.Filled.Menu, "Menu", !busy) { pause() }
                BarButton(null, if (fullscreenTouch) "Hide controls" else "Touch controls") { fullscreenTouch = !fullscreenTouch }
                BarButton(EXIT_FULLSCREEN, "Exit full screen") { applyFullscreen(false) }
                RewindBarButton()
                ForwardBarButton()
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
                forwarding -> "Fast-forward ${settings.fastForward}×"
                rewinding -> "Rewinding"
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
    // Held, not tapped: the game runs backwards for as long as a finger is
    // down. Framed as "Undo" because that is what it is to the player.
    @Composable private fun RewindButton(side: Int) {
        Box(Modifier.size(side.dp).semantics {
            role = Role.Button
            contentDescription = "Undo. Hold to rewind the game."
            stateDescription = if (rewinding) "Rewinding" else "Released"
        }.clip(RoundedCornerShape(28.dp)).background(if (rewinding) Color(0xffffd9a0) else Color(0xffe0b877)).pointerInput(game) {
            hold({ applyRewinding(it) })
        }, contentAlignment = Alignment.Center) { Text("Undo", color = Color(0xff33240c), fontSize = (side * 0.2f).sp, fontWeight = FontWeight.Bold) }
    }
    @Composable private fun ForwardButton(side: Int) {
        Box(Modifier.size(side.dp).semantics {
            role = Role.Button
            contentDescription = "Fast-forward. Hold to skip ahead."
            stateDescription = if (forwarding) "Fast-forwarding" else "Released"
        }.clip(RoundedCornerShape(28.dp)).background(if (forwarding) Color(0xffc9d9ff) else Color(0xff9db4e0)).pointerInput(game) {
            hold({ applyForwarding(it) })
        }, contentAlignment = Alignment.Center) { Text("▶▶", color = Color(0xff10203a), fontSize = (side * 0.24f).sp, fontWeight = FontWeight.Bold) }
    }
    @Composable private fun RewindBarButton() = HoldBarButton("Undo", "Rewinding", rewinding, Icons.Filled.Refresh) { applyRewinding(it) }
    @Composable private fun ForwardBarButton() = HoldBarButton("Fast-forward", "${settings.fastForward}×", forwarding, null) { applyForwarding(it) }
    @Composable private fun HoldBarButton(label: String, active: String, on: Boolean, icon: ImageVector?, apply: (Boolean) -> Unit) {
        TextButton(onClick = {}, modifier = Modifier.pointerInput(game) { hold(apply) }
            .semantics { contentDescription = "$label. Hold to use." }) {
            if (icon != null) {
                Icon(icon, contentDescription = null, Modifier.size(20.dp))
                Spacer(Modifier.width(6.dp))
            }
            Text(if (on) active else label)
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
     * One settings panel, reachable from the shelf and from a paused game. Opening
     * it mid-game is deliberate: picture changes apply live, so you can see what
     * 8:7 or a trimmed edge actually does to the game in front of you.
     */
    @Composable private fun SettingsPanel() {
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.85f)).padding(24.dp), contentAlignment = Alignment.Center) {
            Surface(shape = RoundedCornerShape(28.dp), modifier = Modifier.widthIn(max = 620.dp).fillMaxWidth()) {
                Column(Modifier.padding(24.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("Settings", fontSize = 28.sp, fontWeight = FontWeight.Bold)
                    SectionLabel("Picture")
                    SettingRow("Shape", settings.aspect.label) {
                        settings.aspect = Aspect.entries[(settings.aspect.ordinal + 1) % Aspect.entries.size]
                    }
                    SettingRow("Trim the edges", onOff(settings.trimEdges), "Hides the ${Picture.TRIM} rows a television lost to overscan") {
                        settings.trimEdges = !settings.trimEdges
                    }
                    SettingRow("Scanlines", onOff(settings.scanlines), "A soft dark band between each row, the way a CRT drew them") {
                        settings.scanlines = !settings.scanlines
                    }
                    SectionLabel("Controls")
                    SettingRow("Big controls", onOff(settings.bigControls), "Larger touch targets") { settings.bigControls = !settings.bigControls }
                    SettingRow("Fast-forward speed", "${settings.fastForward}×", "How fast the fast-forward button runs") { settings.cycleFastForward() }
                    SettingRow("Controller buttons", "Set up", "Map A, B, Select and Start for a controller") { showSettings = false; startMapping() }
                    SectionLabel("This device")
                    SettingRow("Problem log", "Open", "What went wrong, and why") { showProblems = true }
                    Spacer(Modifier.height(8.dp))
                    Button(onClick = { showSettings = false }, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text("Done") }
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
                TextButton(onClick = { showSettings = true }) { Text("Settings") }
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
                        TextButton(enabled = !busy, onClick = { showSettings = true }) { Text("Settings") }
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
    // Bigger buttons are easier on a 13" screen, so this is a setting rather than
    // a mode. Narrow widths grow less, because a layout that overflows helps nobody.
    @Composable private fun TouchControls() {
        BoxWithConstraints(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp)) {
            val compact = maxWidth < 600.dp
            val scale = if (!settings.bigControls) 1f else if (compact) 1.15f else 1.4f
            fun size(base: Int) = (base * scale).toInt()
            Column {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                    Dpad(size(144))
                    if (!compact) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { HoldButton("Select", 4, size(64)); HoldButton("Start", 8, size(64)) }
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        ForwardButton(size(64)); RewindButton(size(76)); HoldButton("B", 2, size(68)); HoldButton("A", 1, size(76))
                    }
                }
                if (compact) Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.Center) { HoldButton("Select", 4, size(64)); Spacer(Modifier.width(12.dp)); HoldButton("Start", 8, size(64)) }
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

/**
 * Runs [apply] with true while a finger is down and false when it lifts, however
 * the gesture ends. Rewind and fast-forward are both this shape, and a release
 * that gets missed leaves the game running at the wrong speed.
 */
private suspend fun androidx.compose.ui.input.pointer.PointerInputScope.hold(apply: (Boolean) -> Unit) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        try {
            apply(true)
            do {
                val event = awaitPointerEvent()
                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                change.consume()
            } while (change.pressed)
        } finally { apply(false) }
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
