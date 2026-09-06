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

data class Game(val id: String, val title: String, val added: Long)
data class SaveSlot(val number: Int, val time: Long, val thumbnail: File)

/** Private imported copies survive document-provider moves and permission revocation. */
class Library(private val context: Context) {
    private val root = File(context.filesDir, "library").apply { mkdirs() }
    private val index = File(root, "index.json")
    fun games(): List<Game> {
        if (!index.exists()) return emptyList()
        val items = JSONArray(AtomicFile(index).openRead().bufferedReader().use { it.readText() })
        return (0 until items.length()).map { val g = items.getJSONObject(it); Game(g.getString("id"), g.getString("title"), g.getLong("added")) }.sortedByDescending { it.added }
    }
    fun directory(id: String) = File(root, id).apply { mkdirs() }
    fun rom(game: Game) = File(directory(game.id), "game.nes").readBytes()
    fun readImport(uri: Uri): Pair<String, ByteArray> {
        var title = "My game"
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) title = it.getString(0).substringBeforeLast('.')
        }
        val bytes = context.contentResolver.openInputStream(uri)?.use { boundedRead(it) } ?: error("Could not open this file")
        return title to bytes
    }
    fun add(id: String, title: String, bytes: ByteArray): Game {
        val games = games().toMutableList()
        val existing = games.firstOrNull { it.id == id }
        if (existing != null) return existing
        val game = Game(id, title, System.currentTimeMillis())
        atomic(File(directory(id), "game.nes"), bytes)
        games.add(game)
        val json = JSONArray(); games.forEach { json.put(JSONObject().put("id", it.id).put("title", it.title).put("added", it.added)) }
        atomic(index, json.toString().toByteArray())
        return game
    }
    fun state(game: Game, slot: Int) = File(directory(game.id), if (slot == -1) "auto.state" else "slot-$slot.state")
    fun thumbnail(game: Game, slot: Int) = File(directory(game.id), if (slot == -1) "auto.png" else "slot-$slot.png")
    fun battery(game: Game) = File(directory(game.id), "battery.sav")
    fun slots(game: Game) = (0..9).map { SaveSlot(it, state(game, it).takeIf(File::exists)?.lastModified() ?: 0, thumbnail(game, it)) }
    companion object {
        fun boundedRead(input: InputStream): ByteArray {
            val output = ByteArrayOutputStream()
            val chunk = ByteArray(8192)
            while (true) {
                val count = input.read(chunk)
                if (count < 0) break
                require(output.size() + count <= 16 * 1024 * 1024) { "This file is too large to be an NES game" }
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
