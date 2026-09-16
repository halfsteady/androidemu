package dev.androidemu

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** Inline panels keep controller events in the activity's dispatch window. */
@Composable fun ControllersPanel(
    controllers: List<ConnectedController>,
    selected: ConnectedController?,
    notice: String?,
    onSelect: (Int) -> Unit,
    onPlayer: (Int?) -> Unit,
    onMap: () -> Unit,
    onBack: () -> Unit,
) {
    if (selected == null) {
        Panel("Controllers", "Choose a device to assign its player and set its buttons.", onBack = onBack) {
            for (player in 0..1) {
                val names = controllers.filter { it.player == player }.joinToString { it.name }
                InfoRow("Player ${player + 1}", names.ifEmpty { "No controller assigned" })
            }
            Text("Choose 2 players in the game's own menu to play together. Touch controls always use Player 1.",
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            HorizontalDivider()
            if (controllers.isEmpty()) {
                Text("Connect a USB or Bluetooth controller or keyboard. It will appear here automatically.")
            } else {
                SectionLabel("Connected devices")
                controllers.forEach { controller ->
                    val duplicate = controllers.count { it.name == controller.name } > 1
                    ValueRow(controller.name, controller.playerLabel,
                        (if (controller.gamepad) "Gamepad" else "Keyboard / remote") +
                            (if (duplicate) " · Device ${controller.id}" else "") + " · Select to configure") {
                        onSelect(controller.id)
                    }
                }
            }
            notice?.let { Text(it, color = MaterialTheme.colorScheme.primary) }
            SecondaryAction("Done", Modifier.fillMaxWidth(), onClick = onBack)
        }
    } else {
        Panel(selected.name, "${selected.playerLabel} · ${if (selected.gamepad) "Gamepad" else "Keyboard / remote"}",
            onBack = onBack, maxWidth = 620.dp) {
            ChoiceRow("Controls", listOf("Player 1", "Player 2", "Not assigned"), selected.player ?: 2) {
                onPlayer(it.takeIf { choice -> choice < 2 })
            }
            ChoiceNote("The player choice is remembered for this device. Devices on the same player share its controls.")
            HorizontalDivider()
            SectionLabel("Buttons for ${selected.playerLabel.lowercase()}")
            Text(if (selected.customMapping) "Custom buttons saved for ${selected.name}." else "Using the default buttons.")
            PrimaryAction("Set buttons", Modifier.fillMaxWidth(), enabled = selected.player != null, onClick = onMap)
            Text("Set A, B, Select and Start on this device. Directions use its D-pad or stick; keyboards use arrow keys.",
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (selected.player == null) ChoiceNote("Choose a player to set buttons and use this device in a game.")
            notice?.let { Text(it, color = MaterialTheme.colorScheme.primary) }
            SecondaryAction("Back to controllers", Modifier.fillMaxWidth(), onClick = onBack)
        }
    }
}
