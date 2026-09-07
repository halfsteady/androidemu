package dev.androidemu

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.InputStream
import java.io.ByteArrayOutputStream
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * A game on the shelf. [played] and [seconds] arrived after the first release, so
 * they are read with defaults: an index written by an older build still loads.
 */
data class Game(
    val id: String,
    val title: String,
    val added: Long,
    val played: Long = 0,
    val seconds: Long = 0,
)
data class SaveSlot(val number: Int, val time: Long, val thumbnail: File)

/** Private imported copies survive document-provider moves and permission revocation. */
class Library(private val context: Context) {
    private val root = File(context.filesDir, "library").apply { mkdirs() }
    private val index = File(root, "index.json")
    private val problems = File(root, "problems.log")
    /** Most recently played first, falling back to when it was added. */
    fun games(): List<Game> = read().sortedByDescending { maxOf(it.played, it.added) }
    private fun read(): List<Game> {
        if (!index.exists()) return emptyList()
        val items = JSONArray(AtomicFile(index).openRead().bufferedReader().use { it.readText() })
        return (0 until items.length()).map {
            val g = items.getJSONObject(it)
            Game(g.getString("id"), g.getString("title"), g.getLong("added"), g.optLong("played"), g.optLong("seconds"))
        }
    }
    private fun write(games: List<Game>) {
        val json = JSONArray()
        games.forEach {
            json.put(
                JSONObject().put("id", it.id).put("title", it.title).put("added", it.added)
                    .put("played", it.played).put("seconds", it.seconds)
            )
        }
        atomic(index, json.toString().toByteArray())
    }
    /**
     * Records a finished stretch of play. Called on every pause and background, so
     * it reads the index back rather than trusting a stale copy held by the caller.
     */
    fun record(game: Game, seconds: Long) {
        val games = read().toMutableList()
        val at = games.indexOfFirst { it.id == game.id }
        if (at < 0) return
        val existing = games[at]
        games[at] = existing.copy(played = System.currentTimeMillis(), seconds = existing.seconds + seconds.coerceAtLeast(0))
        write(games)
    }
    fun directory(id: String) = File(root, id).apply { mkdirs() }
    fun rom(game: Game) = File(directory(game.id), "game.nes").readBytes()
    fun readImport(uri: Uri): Pair<String, ByteArray> {
        var title = "My game"
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) title = it.getString(0).substringBeforeLast('.')
        }
        return title to readBytes(uri)
    }
    fun add(id: String, title: String, bytes: ByteArray): Game {
        val games = read().toMutableList()
        val existing = games.firstOrNull { it.id == id }
        if (existing != null) return existing
        val game = Game(id, title, System.currentTimeMillis())
        atomic(File(directory(id), "game.nes"), bytes)
        games.add(game)
        write(games)
        return game
    }
    fun state(game: Game, slot: Int) = File(directory(game.id), if (slot == -1) "auto.state" else "slot-$slot.state")
    fun thumbnail(game: Game, slot: Int) = File(directory(game.id), if (slot == -1) "auto.png" else "slot-$slot.png")
    fun battery(game: Game) = File(directory(game.id), "battery.sav")
    fun slots(game: Game) = (0..9).map { SaveSlot(it, state(game, it).takeIf(File::exists)?.lastModified() ?: 0, thumbnail(game, it)) }
    /** Box art chosen by hand. It outranks the saved screenshot on the shelf. */
    fun art(game: Game) = File(directory(game.id), "art.png")
    /** The picture the shelf should show: chosen art, else the last saved moment. */
    fun cover(game: Game): File? = art(game).takeIf(File::exists) ?: thumbnail(game, -1).takeIf(File::exists)
    fun readBytes(uri: Uri): ByteArray =
        context.contentResolver.openInputStream(uri)?.use { boundedRead(it) } ?: error("Could not open this file")

    /**
     * Kid mode says "this game didn't work" and nothing else, so the real reason
     * has to land somewhere a grown-up can read it later. Bounded, because a
     * failure that repeats every frame would otherwise fill the device.
     */
    fun logProblem(label: String, detail: String) {
        runCatching {
            val stamp = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US).format(Date())
            val kept = if (problems.exists()) problems.readLines().takeLast(PROBLEM_LINES - 1) else emptyList()
            atomic(problems, (kept + "$stamp  $label — $detail").joinToString("\n").toByteArray())
        }
    }
    fun problems(): List<String> = if (problems.exists()) runCatching { problems.readLines().reversed() }.getOrDefault(emptyList()) else emptyList()
    companion object {
        private const val PROBLEM_LINES = 200
        fun boundedRead(input: InputStream): ByteArray {
            val output = ByteArrayOutputStream()
            val chunk = ByteArray(8192)
            while (true) {
                val count = input.read(chunk)
                if (count < 0) break
                require(output.size() + count <= 16 * 1024 * 1024) { "This file is too large to be a game" }
                output.write(chunk, 0, count)
            }
            return output.toByteArray()
        }
        fun atomic(file: File, bytes: ByteArray) {
            val atomic = AtomicFile(file); val stream = atomic.startWrite()
            try { stream.write(bytes); atomic.finishWrite(stream) } catch (e: Exception) { atomic.failWrite(stream); throw e }
        }
    }
}
