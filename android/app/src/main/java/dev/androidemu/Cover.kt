package dev.androidemu

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import java.io.File

/**
 * A game's picture: chosen box art, else the last saved moment, else its initial
 * on a tinted card. Cropped rather than fitted, so a shelf of mixed sources still
 * reads as one grid of tiles instead of a row of letterboxed rectangles.
 */
@Composable fun Cover(title: String, file: File?, modifier: Modifier = Modifier, revision: Int = 0) {
    // Keyed on the timestamp and on the revision the caller bumps when it
    // replaces a file, since a lazy grid may otherwise skip an unchanged row.
    val image = remember(file?.path, file?.lastModified(), revision) {
        file?.takeIf { it.exists() }?.let { runCatching { BitmapFactory.decodeFile(it.path) }.getOrNull() }?.asImageBitmap()
    }
    Box(
        modifier.background(Brush.linearGradient(listOf(Color(0xff3b5a43), Color(0xff21301f)))),
        contentAlignment = Alignment.Center,
    ) {
        if (image != null) Image(image, "$title cover", Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        else Text(title.take(1).uppercase(), fontSize = 76.sp, color = Color(0xffd9efc0), fontWeight = FontWeight.Black)
    }
}
