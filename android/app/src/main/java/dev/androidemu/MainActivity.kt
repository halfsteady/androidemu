package dev.androidemu

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.text.DateFormat
import java.util.Date
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class MainActivity : ComponentActivity() {
    private lateinit var library: Library
    private lateinit var surface: GameSurface
    private lateinit var input: ControllerInput
    private var games by mutableStateOf(emptyList<Game>())
    private var game by mutableStateOf<Game?>(null)
    private var paused by mutableStateOf(true)
    private var busy by mutableStateOf(false)
    private var message by mutableStateOf<String?>(null)
    private var slots by mutableStateOf(emptyList<SaveSlot>())
    private var showSlots by mutableStateOf(false)
    private var overwrite by mutableStateOf<Int?>(null)
    @Volatile private var backgrounded = false
    @Volatile private var activeGame: Game? = null
    private val importGame = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) work("Adding game") {
            val (title, bytes) = library.readImport(uri)
            surface.loaded = false
            try {
                val id = Native.load(bytes)
                val selected = library.add(id, title, bytes)
                openLoaded(selected)
            } catch (e: Exception) { runOnUiThread { game = null }; throw e }
        }
    }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        library = Library(this)
        input = ControllerInput(this) { if (game != null) pause(); message = "Controller disconnected. Your game is paused." }
        surface = GameSurface(this, input::buttons) { message = "This game couldn't continue. $it"; paused = true; busy = false }
        runCatching { games = library.games() }.onFailure { message = "Your library couldn't be opened. Your game files are still stored on this device." }
        setContent { MaterialTheme(colorScheme = darkColorScheme(primary = Color(0xffb9e38c), background = Color(0xff111813), surface = Color(0xff1d2820), onSurface = Color(0xffedf4e9))) { App() } }
    }
    private fun work(label: String, action: () -> Unit) {
        if (busy) return
        busy = true; surface.playing = false; input.clear()
        surface.task {
            try { Native.audio(false); action() }
            catch (e: Exception) { runOnUiThread { paused = true; message = "$label didn't finish. ${e.message ?: "Please try again."}" } }
            finally { runOnUiThread { busy = false } }
        }
    }
    private fun openLoaded(selected: Game) {
        val battery = library.battery(selected)
        var warning: String? = null
        if (battery.exists()) runCatching { Native.restore(battery.readBytes(), true) }.onFailure { warning = "The battery save couldn't be opened. Your saved moments are still available." }
        val auto = library.state(selected, -1)
        if (auto.exists()) runCatching { Native.restore(auto.readBytes(), false) }.onFailure { warning = "Your automatic save couldn't be opened. The game started from its battery save; manual save slots are still available." }
        activeGame = selected
        surface.loaded = true
        runOnUiThread {
            game = selected; games = library.games(); slots = library.slots(selected); message = warning
            if (!backgrounded && warning == null) resumeGame() else paused = true
        }
    }
    private fun open(selected: Game) = work("Opening game") {
        surface.loaded = false
        try { Native.load(library.rom(selected)); openLoaded(selected) }
        catch (e: Exception) { runOnUiThread { game = null }; throw e }
    }
    private fun resumeGame() {
        showSlots = false; paused = false
        surface.isFocusableInTouchMode = true; surface.requestFocus()
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        surface.task { if (!backgrounded) { Native.audio(true); surface.playing = true } }
    }
    private fun pause() {
        surface.playing = false; paused = true; input.clear()
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val selected = game ?: return
        if (!surface.loaded) return
        work("Saving progress") { save(selected, -1); runOnUiThread { slots = library.slots(selected) } }
    }
    private fun save(selected: Game, slot: Int) {
        val bytes = Native.snapshot(false)
        Library.atomic(library.state(selected, slot), bytes)
        val battery = Native.snapshot(true)
        if (battery.isNotEmpty()) Library.atomic(library.battery(selected), battery)
        val pixels = ByteBuffer.allocateDirect(256 * 240 * 4)
        Native.frame(pixels, 0, 0, false)
        val bitmap = Bitmap.createBitmap(256, 240, Bitmap.Config.ARGB_8888)
        pixels.rewind(); bitmap.copyPixelsFromBuffer(pixels)
        val out = ByteArrayOutputStream(); bitmap.compress(Bitmap.CompressFormat.PNG, 100, out); bitmap.recycle()
        Library.atomic(library.thumbnail(selected, slot), out.toByteArray())
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
        val done = CountDownLatch(1)
        surface.task {
            try { Native.audio(false); val selected = activeGame; if (selected != null && surface.loaded) save(selected, -1) }
            catch (e: Exception) { runOnUiThread { message = "Automatic save failed. ${e.message}" } }
            finally { done.countDown() }
        }
        done.await(1500, TimeUnit.MILLISECONDS)
        super.onPause()
    }
    override fun onResume() { super.onResume(); backgrounded = false }
    override fun onDestroy() { input.close(); super.onDestroy() }
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (game != null && !paused && input.key(event)) return true
        if (game != null && keyCode == KeyEvent.KEYCODE_BUTTON_START && paused && !busy) { resumeGame(); return true }
        return super.onKeyDown(keyCode, event)
    }
    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (game != null && !paused && input.key(event)) return true
        return super.onKeyUp(keyCode, event)
    }
    override fun onGenericMotionEvent(event: MotionEvent) = if (game != null && !paused && input.motion(event)) true else super.onGenericMotionEvent(event)

    @Composable private fun App() {
        BackHandler(game != null) { if (!paused) pause() else if (!busy) { if (showSlots) showSlots = false else game = null } }
        Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).safeDrawingPadding()) {
            // Keep the GL thread attached while the library is visible so imports
            // and state operations have a single serialized execution queue.
            Column(Modifier.fillMaxSize()) {
                if (game != null) Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(game!!.title, Modifier.weight(1f), fontWeight = FontWeight.Bold, maxLines = 1)
                    TextButton(enabled = !busy, onClick = { pause() }) { Text("Pause & save") }
                }
                AndroidView(factory = { surface }, modifier = Modifier.weight(1f).fillMaxWidth())
                if (game != null) TouchControls()
            }
            if (game == null) Shelf()
            if (game != null && paused) PausePanel()
            if (busy) Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.65f)), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        }
        message?.let { text -> AlertDialog(onDismissRequest = { message = null }, title = { Text("A quick update") }, text = { Text(text) }, confirmButton = { TextButton(onClick = { message = null }) { Text("Got it") } }) }
        overwrite?.let { slot -> AlertDialog(onDismissRequest = { overwrite = null }, title = { Text("Replace slot ${slot + 1}?") }, text = { Text("This replaces the progress saved in this slot. Your other slots stay available.") }, confirmButton = { TextButton(onClick = { overwrite = null; saveSlot(slot) }) { Text("Replace save") } }, dismissButton = { TextButton(onClick = { overwrite = null }) { Text("Keep it") } }) }
    }
    @Composable private fun Shelf() {
        Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).padding(24.dp)) {
            Text("AMELIA’S NES", color = MaterialTheme.colorScheme.primary, letterSpacing = 3.sp, fontSize = 13.sp, fontWeight = FontWeight.Bold)
            Text(BuildConfig.VERSION_NAME, color = Color(0xffabb8a9), fontSize = 11.sp)
            Spacer(Modifier.height(12.dp))
            Text("Your next adventure", fontSize = 32.sp, fontWeight = FontWeight.Bold)
            Text("Pick a game. Play a little. Come back anytime.", color = Color(0xffabb8a9), modifier = Modifier.padding(top = 8.dp, bottom = 20.dp))
            Button(enabled = !busy, onClick = { importGame.launch(arrayOf("*/*")) }) { Text("＋ Add a game") }
            if (games.isEmpty()) {
                Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text("A shelf full of possibilities", fontSize = 23.sp, fontWeight = FontWeight.SemiBold)
                        Text("Add an NES (.nes) file from your tablet to begin.\nGames stay on this device. No account needed.", modifier = Modifier.padding(16.dp), color = Color(0xffabb8a9))
                    }
                }
            } else LazyVerticalGrid(columns = GridCells.Adaptive(220.dp), contentPadding = PaddingValues(top = 24.dp), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                items(games, key = { it.id }) { selected ->
                    Card(onClick = { open(selected) }, enabled = !busy, shape = RoundedCornerShape(24.dp)) {
                        val thumb = library.thumbnail(selected, -1)
                        val image = remember(selected.id, thumb.lastModified()) { if (thumb.exists()) BitmapFactory.decodeFile(thumb.path)?.asImageBitmap() else null }
                        Box(Modifier.fillMaxWidth().aspectRatio(4f/3f).background(Brush.linearGradient(listOf(Color(0xff3b5a43), Color(0xff243349)))), contentAlignment = Alignment.Center) {
                            if (image != null) Image(image, "Last saved moment", Modifier.fillMaxSize())
                            else Text(selected.title.take(1).uppercase(), fontSize = 80.sp, color = Color(0xffd9efc0), fontWeight = FontWeight.Black)
                        }
                        Column(Modifier.padding(18.dp)) {
                            Text(selected.title, fontWeight = FontWeight.Bold, fontSize = 19.sp)
                            Text(if (library.state(selected, -1).exists()) "Resume your adventure  →" else "Ready to play  →", color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(top = 8.dp))
                        }
                    }
                }
            }
        }
    }
    @Composable private fun PausePanel() {
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.8f)).padding(24.dp), contentAlignment = Alignment.Center) {
            Surface(shape = RoundedCornerShape(28.dp), modifier = Modifier.widthIn(max = 720.dp).fillMaxWidth()) {
                Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(if (showSlots) "Your saved moments" else "Take your time", fontSize = 28.sp, fontWeight = FontWeight.Bold)
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
                    OutlinedButton(enabled = !busy, onClick = { showSlots = !showSlots }, modifier = Modifier.fillMaxWidth()) { Text(if (showSlots) "Back to pause" else "Save & load moments") }
                    TextButton(enabled = !busy, onClick = { game = null; showSlots = false }) { Text("Back to your shelf") }
                }
            }
        }
    }
    private fun pulseTouch(bit: Int) {
        if (paused) return
        input.touch = input.touch or bit
        surface.postDelayed({ input.touch = input.touch and bit.inv() }, 150)
    }
    @Composable private fun TouchControls() {
        BoxWithConstraints(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp)) {
            val compact = maxWidth < 600.dp
            Column {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                    Dpad()
                    if (!compact) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { HoldButton("Select", 4, 64); HoldButton("Start", 8, 64) }
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) { HoldButton("B", 2, 68); HoldButton("A", 1, 76) }
                }
                if (compact) Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.Center) { HoldButton("Select", 4, 64); Spacer(Modifier.width(12.dp)); HoldButton("Start", 8, 64) }
            }
        }
    }
    @Composable private fun Dpad() {
            Box(Modifier.size(144.dp).semantics {
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
            }, contentAlignment = Alignment.Center) { Text("✚", fontSize = 74.sp, color = Color(0xffb6c9af)) }
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
        }, contentAlignment = Alignment.Center) { Text(label, color = Color(0xff203018), fontSize = if (label.length == 1) 28.sp else 13.sp, fontWeight = FontWeight.Bold) }
    }
}
