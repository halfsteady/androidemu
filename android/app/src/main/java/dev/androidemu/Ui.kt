package dev.androidemu

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The things that make this look like one app rather than several.
 *
 * Material names most of the colours, but not a recessed track, a raised key or
 * a control that cannot be used, and those were the places where a second
 * palette crept in: the time control and the skip-back keys were a cold
 * blue-grey against a warm green everything else. Every colour here is in the
 * one family, and the two directions of time are told apart by hue *within* it
 * rather than by belonging to a different theme.
 *
 * The corner and height scales are here for the same reason. Five different
 * radii and a button height per call site is not a style, it is an accident.
 */
object Ui {
    /** Three corners: a row, a tile, a panel. Anything else is a fourth accident. */
    val cornerSmall = 14.dp
    val cornerMedium = 20.dp
    val cornerLarge = 28.dp

    /**
     * Touch targets. A 13" tablet has the room, and one of the two people this
     * is for is small enough that a 40 dp target is a miss.
     */
    val primaryHeight = 60.dp
    val secondaryHeight = 52.dp
    val quietHeight = 46.dp

    // The three greens the whole app is built out of, named once and then reused
    // both by the Material scheme below and by the tokens under it — so a
    // touch button and the theme's primary cannot drift apart.
    private val leaf = Color(0xffb9e38c)
    private val onLeaf = Color(0xff17300c)
    private val raised = Color(0xff2a3a2e)
    private val onRaised = Color(0xffc3d1bd)

    /**
     * The colour scheme, built here rather than in the activity so that every
     * colour in the app is decided in one file. Every slot a Material component
     * actually reads is set: the defaults are purple-tinted, and any one left
     * unset shows up as an off-hue label or dialog against this green.
     */
    val scheme = darkColorScheme(
        primary = leaf,
        onPrimary = onLeaf,
        background = Color(0xff111813),
        onBackground = Color(0xffedf4e9),
        surface = Color(0xff1d2820),
        onSurface = Color(0xffedf4e9),
        surfaceVariant = raised,
        onSurfaceVariant = onRaised,
        surfaceContainer = Color(0xff1d2820),
        surfaceContainerHigh = Color(0xff243128),
        outline = Color(0xff6d7f68),
        error = Color(0xffffb4a6),
        onError = Color(0xff5f1409),
    )

    /** Recessed: the time track, and the back of anything sunk into a panel. */
    val well = Color(0xff141d17)

    /** A line drawn on the well — the centre notch time springs back to. */
    val wellEdge = Color(0xff3b4d3e)

    /** A glyph drawn on the well — the direction chevrons. */
    val wellMark = Color(0xff5f7359)

    /** A raised control: the directional pad, a save slot, a menu tile. */
    val keyFace = raised
    val keyGlyph = onRaised

    /** A control that is deliberately inert, and still has to be readable. */
    val keyDim = Color(0xff1b241d)
    val keyDimGlyph = Color(0xff54654f)

    /**
     * The two directions of time. Forward is the app's own green; back is amber,
     * which the end-of-tape notice also speaks in, so "backwards" has one colour
     * wherever it appears.
     */
    val forward = leaf
    val backward = Color(0xfff0d49a)
    val onTime = Color(0xff152210)
    val noticeBack = Color(0xff2b2410)
    val noticeText = Color(0xfff0d49a)

    /** The touch buttons: the one thing meant to look pressable from a metre away. */
    val button = leaf
    val buttonHeld = Color(0xffd6f6ac)
    val onButton = onLeaf

    /** What a panel dims the app behind it with. Green-black, not black. */
    val scrim = Color(0xd90a120c)

    /** What a control drawn over the picture sits on. */
    val chrome = Color(0xcc0a120c)
}

/**
 * The main thing to do here. One per panel, full width, and unmissable.
 *
 * [height] gives way in a header row, where a 60 dp button next to a 52 dp one
 * reads as a mistake rather than as emphasis.
 */
@Composable fun PrimaryAction(
    label: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    enabled: Boolean = true,
    height: Dp = Ui.primaryHeight,
    onClick: () -> Unit,
) {
    Button(
        onClick = onClick,
        enabled = enabled,
        shape = RoundedCornerShape(Ui.cornerMedium),
        modifier = modifier.heightIn(min = height),
    ) {
        if (icon != null) { Icon(icon, null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)) }
        Text(label, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
    }
}

/** A real choice, just not the main one: leaving, going back, cancelling. */
@Composable fun SecondaryAction(
    label: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    enabled: Boolean = true,
    height: Dp = Ui.secondaryHeight,
    onClick: () -> Unit,
) {
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        shape = RoundedCornerShape(Ui.cornerMedium),
        modifier = modifier.heightIn(min = height),
    ) {
        if (icon != null) { Icon(icon, null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)) }
        Text(label, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
    }
}

/**
 * A small action inside something else — a save slot, a card. [danger] is the
 * only way a destructive action is ever drawn, so "delete" never arrives wearing
 * the same clothes as "load".
 */
@Composable fun QuietAction(
    label: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    danger: Boolean = false,
    compact: Boolean = false,
    onClick: () -> Unit,
) {
    TextButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = if (compact) 40.dp else Ui.quietHeight),
        contentPadding = if (compact) PaddingValues(horizontal = 10.dp, vertical = 4.dp) else ButtonDefaults.TextButtonContentPadding,
        colors = if (danger) ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error)
        else ButtonDefaults.textButtonColors(),
    ) {
        Text(label, fontWeight = FontWeight.SemiBold)
    }
}

/**
 * One entry in a menu of places to go. Icon then label, on a raised face, so a
 * menu is a row of equals rather than a paragraph of text buttons in which the
 * important one is whichever happens to be first.
 */
@Composable fun ActionTile(
    icon: ImageVector,
    label: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    Surface(
        onClick = onClick,
        enabled = enabled,
        shape = RoundedCornerShape(Ui.cornerMedium),
        color = Ui.keyFace,
        contentColor = MaterialTheme.colorScheme.onSurface,
        modifier = modifier.heightIn(min = 64.dp),
    ) {
        Row(
            Modifier.padding(horizontal = 18.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Icon(icon, null, Modifier.size(22.dp), tint = MaterialTheme.colorScheme.primary)
            Text(label, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
        }
    }
}

/**
 * The frame every panel is drawn in: the same scrim, the same card, the same
 * place for a title, and a back arrow when there is somewhere to go back to.
 *
 * A panel that goes deeper — save states out of the pause menu — is a panel with
 * a back arrow, not the same panel with a different title and a toggle button.
 */
@Composable fun Panel(
    title: String,
    subtitle: String? = null,
    onBack: (() -> Unit)? = null,
    maxWidth: Dp = 620.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    Box(Modifier.fillMaxSize().background(Ui.scrim).padding(20.dp), contentAlignment = Alignment.Center) {
        Surface(shape = RoundedCornerShape(Ui.cornerLarge), modifier = Modifier.widthIn(max = maxWidth).fillMaxWidth()) {
            Column(
                Modifier.padding(horizontal = 24.dp, vertical = 22.dp).verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (onBack != null) {
                        IconButton(onClick = onBack, modifier = Modifier.size(44.dp)) {
                            Icon(BACK, "Back", Modifier.size(24.dp))
                        }
                        Spacer(Modifier.width(6.dp))
                    }
                    Text(title, fontSize = 27.sp, fontWeight = FontWeight.Bold)
                }
                if (subtitle != null) Text(subtitle, color = MaterialTheme.colorScheme.onSurfaceVariant)
                content()
            }
        }
    }
}

/** The heading over a group of settings. */
@Composable fun SectionLabel(text: String) {
    Text(
        text.uppercase(),
        Modifier.padding(top = 14.dp, bottom = 2.dp),
        color = MaterialTheme.colorScheme.primary,
        fontSize = 12.sp,
        letterSpacing = 2.sp,
        fontWeight = FontWeight.Bold,
    )
}

/** A labelled row of choices. Chips rather than a cycling row, so the preview is one tap from any option. */
@Composable fun ChoiceRow(label: String, options: List<String>, selected: Int, onPick: (Int) -> Unit) {
    Column(Modifier.padding(vertical = 2.dp)) {
        Text(label, Modifier.padding(start = 4.dp, bottom = 6.dp), fontWeight = FontWeight.SemiBold)
        Row(
            Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            options.forEachIndexed { index, option ->
                FilterChip(selected = index == selected, onClick = { onPick(index) }, label = { Text(option) })
            }
        }
    }
}

/** The note under a choice, explaining what it does. */
@Composable fun ChoiceNote(text: String) {
    Text(
        text,
        Modifier.padding(start = 4.dp, bottom = 4.dp),
        fontSize = 12.sp,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** Label on the left, current value on the right, the whole row a target. */
@Composable fun ValueRow(label: String, value: String, hint: String? = null, enabled: Boolean = true, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        enabled = enabled,
        color = Color.Transparent,
        shape = RoundedCornerShape(Ui.cornerSmall),
        modifier = Modifier.fillMaxWidth().heightIn(min = Ui.quietHeight),
    ) { RowBody(label, hint) { Text(value, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold) } }
}

/**
 * The same row, for something that is only being reported. It is not clickable,
 * because a row that highlights under a finger and then does nothing is a lie
 * about what is on offer.
 */
@Composable fun InfoRow(label: String, value: String, hint: String? = null) {
    Box(Modifier.fillMaxWidth().heightIn(min = Ui.quietHeight)) {
        RowBody(label, hint) { Text(value, color = MaterialTheme.colorScheme.onSurfaceVariant, fontWeight = FontWeight.Bold) }
    }
}

/** A row for something that is on or off, drawn as the switch it is. */
@Composable fun SwitchRow(label: String, checked: Boolean, hint: String? = null, onToggle: () -> Unit) {
    Surface(
        onClick = onToggle,
        color = Color.Transparent,
        shape = RoundedCornerShape(Ui.cornerSmall),
        modifier = Modifier.fillMaxWidth().heightIn(min = Ui.quietHeight),
    ) { RowBody(label, hint) { Switch(checked = checked, onCheckedChange = null) } }
}

@Composable private fun RowBody(label: String, hint: String?, trailing: @Composable () -> Unit) {
    Row(
        Modifier.padding(horizontal = 4.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(label, fontWeight = FontWeight.SemiBold)
            if (hint != null) Text(hint, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        trailing()
    }
}


/**
 * Icons, all from one place and drawn at one weight. The set is small on
 * purpose: a menu of eight different icon styles is a menu nobody can scan.
 */
private fun icon(name: String, path: String): ImageVector = ImageVector.Builder(
    name = name,
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).addPath(PathParser().parsePathString(path).toNodes(), fill = SolidColor(Color.White)).build()

val ENTER_FULLSCREEN = icon(
    "EnterFullscreen",
    "M7,14H5v5h5v-2H7V14zM5,10h2V7h3V5H5V10zM17,17h-3v2h5v-5h-2V17zM14,5v2h3v3h2V5H14z",
)
val EXIT_FULLSCREEN = icon(
    "ExitFullscreen",
    "M5,16h3v3h2v-5H5V16zM8,8H5v2h5V5H8V8zM14,19h2v-3h3v-2h-5V19zM16,8V5h-2v5h5V8H16z",
)
val MENU = icon("Menu", "M3,18h18v-2H3V18zM3,13h18v-2H3V13zM3,6v2h18V6H3z")
val BACK = icon("Back", "M20,11H7.83l5.59,-5.59L12,4l-8,8 8,8 1.41,-1.41L7.83,13H20v-2z")
val MORE = icon(
    "More",
    "M12,8c1.1,0 2,-0.9 2,-2s-0.9,-2 -2,-2 -2,0.9 -2,2 0.9,2 2,2zM12,10c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 " +
        "2,-0.9 2,-2 -0.9,-2 -2,-2zM12,16c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2z",
)
val ADD = icon("Add", "M19,13h-6v6h-2v-6H5v-2h6V5h2v6h6V13z")
val PLAY = icon("Play", "M8,5v14l11,-7z")
val SLOTS = icon(
    "Slots",
    "M3,13h2v-2H3V13zM3,17h2v-2H3V17zM3,9h2V7H3V9zM7,13h14v-2H7V13zM7,17h14v-2H7V17zM7,7v2h14V7H7z",
)
val CAMERA = icon(
    "Camera",
    "M9,3L7.17,5H4C2.9,5 2,5.9 2,7v12c0,1.1 0.9,2 2,2h16c1.1,0 2,-0.9 2,-2V7c0,-1.1 -0.9,-2 -2,-2h-3.17L15,3H9z" +
        "M12,18c-2.76,0 -5,-2.24 -5,-5s2.24,-5 5,-5 5,2.24 5,5 -2.24,5 -5,5z",
)
val TUNE = icon(
    "Tune",
    "M3,17v2h6v-2H3zM3,5v2h10V5H3zM13,21v-2h8v-2h-8v-2h-2v6H13zM7,9v2H3v2h4v2h2V9H7zM21,13v-2H11v2H21z" +
        "M15,9h2V7h4V5h-4V3h-2V9z",
)
val SHELF = icon(
    "Shelf",
    "M4,3h6v8H4V3zM14,3h6v8h-6V3zM4,13h6v8H4V13zM14,13h6v8h-6V13z",
)
val GAMEPAD = icon(
    "Gamepad",
    "M21,6H3C1.9,6 1,6.9 1,8v8c0,1.1 0.9,2 2,2h18c1.1,0 2,-0.9 2,-2V8C23,6.9 22.1,6 21,6zM11,13H8v3H6v-3H3v-2h3V8h2v3h3V13z" +
        "M15.5,15c-0.83,0 -1.5,-0.67 -1.5,-1.5S14.67,12 15.5,12s1.5,0.67 1.5,1.5S16.33,15 15.5,15zM19.5,11c-0.83,0 -1.5,-0.67 " +
        "-1.5,-1.5S18.67,8 19.5,8 21,8.67 21,9.5 20.33,11 19.5,11z",
)
