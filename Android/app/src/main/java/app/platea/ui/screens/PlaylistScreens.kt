package app.platea.ui.screens

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.PlaylistPlay
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.platea.R
import app.platea.data.Api
import app.platea.data.PlaylistDetail
import app.platea.data.PlaylistList
import app.platea.data.PlaylistSummary
import app.platea.data.Session
import app.platea.ui.LocalNav
import app.platea.ui.components.EmptyBox
import app.platea.ui.components.PrimaryButton
import app.platea.ui.components.ResContent
import app.platea.ui.components.RoundIconButton
import app.platea.ui.components.SecondaryButton
import app.platea.ui.components.focusRing
import app.platea.ui.components.pagePadding
import app.platea.ui.components.pluralRes
import app.platea.ui.components.posterMeta
import app.platea.ui.components.rememberLoad
import app.platea.ui.theme.tokens
import app.platea.util.Fmt
import coil.compose.AsyncImage
import kotlinx.coroutines.launch

@kotlinx.serialization.Serializable
data class PlaylistPatch(val name: String? = null, val shared: Boolean? = null)

@kotlinx.serialization.Serializable
data class MoveBody(@kotlinx.serialization.SerialName("entry_id") val entryId: String, val index: Int)

@Composable
fun PlaylistsScreen() {
    val lists = rememberLoad("playlists") { Api.get<PlaylistList>("/api/v1/playlists").items }
    val nav = LocalNav.current
    val scope = rememberCoroutineScope()
    var creating by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        PageTitle(stringResource(R.string.nav_playlists))
        Row(Modifier.padding(horizontal = pagePadding(), vertical = 4.dp)) {
            SecondaryButton(stringResource(R.string.playlist_new), Icons.Filled.Add) { creating = true }
        }
        ResContent(lists) { ls ->
            if (ls.isEmpty()) {
                EmptyBox(stringResource(R.string.playlist_empty_list))
                return@ResContent
            }
            LazyVerticalGrid(
                columns = gridCells(),
                contentPadding = PaddingValues(start = pagePadding(), end = pagePadding(), top = 12.dp, bottom = 32.dp),
                horizontalArrangement = Arrangement.spacedBy(gridSpacing()),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                items(ls, key = { it.id }) { p -> PlaylistCard(p) { nav.playlist(p.id) } }
            }
        }
    }
    if (creating) {
        NameDialog(stringResource(R.string.playlist_new), "", onClose = { creating = false }) { name ->
            scope.launch {
                runCatching { Api.send("/api/v1/playlists", "POST", NewPlaylistBody(name, emptyList())) }
                lists.reload()
            }
        }
    }
}

@Composable
private fun PlaylistCard(p: PlaylistSummary, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(10.dp)
    Column(Modifier.fillMaxWidth().clickable(interactionSource = interaction, indication = null, onClick = onClick)) {
        Box(Modifier.fillMaxWidth().aspectRatio(2f / 3f).focusRing(interaction, shape).clip(shape).background(tokens.surface2), contentAlignment = Alignment.Center) {
            if (p.poster != null) AsyncImage(Api.image(p.poster, 320), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
            else Icon(Icons.AutoMirrored.Filled.PlaylistPlay, null, tint = tokens.inkSubtle)
        }
        Spacer(Modifier.padding(top = 6.dp))
        Text(p.name, style = MaterialTheme.typography.labelLarge, color = tokens.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            listOfNotNull(pluralRes(R.plurals.titles, p.count), if (!p.mine) p.owner?.let { stringResource(R.string.playlist_by, it) } else if (p.shared) stringResource(R.string.playlist_shared) else null).joinToString(" · "),
            style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle, maxLines = 1,
        )
    }
}

@Composable
fun PlaylistScreen(id: String) {
    val loader = rememberLoad("playlist", id) { Api.get<PlaylistDetail>("/api/v1/playlists/$id") }
    val nav = LocalNav.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var dialog by remember { mutableStateOf<String?>(null) }
    fun act(block: suspend () -> Unit) = scope.launch {
        runCatching { block() }.onFailure { Toast.makeText(context, Api.message(it, context.getString(R.string.error_network)), Toast.LENGTH_SHORT).show() }
        loader.reload()
    }
    ResContent(loader) { pl ->
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 32.dp)) {
            item("head") {
                PageTitle(
                    pl.name,
                    listOfNotNull(pluralRes(R.plurals.titles, pl.items.size), if (pl.runtimeMs > 0) Fmt.duration(pl.runtimeMs) else null, if (!pl.mine) pl.owner?.let { stringResource(R.string.playlist_by, it) } else null).joinToString(" · "),
                    back = true,
                )
                Row(Modifier.padding(horizontal = pagePadding(), vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (pl.items.isNotEmpty() && Session.me?.permissions?.expired != true) {
                        PrimaryButton(stringResource(R.string.play_all), Icons.Filled.PlayArrow) { nav.play(pl.items[0].id, playlistId = pl.id, index = 0) }
                    }
                    if (pl.mine) {
                        RoundIconButton(Icons.Filled.Share, stringResource(if (pl.shared) R.string.playlist_unshare else R.string.playlist_share), pl.shared) {
                            act { Api.send("/api/v1/playlists/$id", "PATCH", PlaylistPatch(shared = !pl.shared)) }
                        }
                        RoundIconButton(Icons.Filled.Edit, stringResource(R.string.rename)) { dialog = "rename" }
                        RoundIconButton(Icons.Filled.Delete, stringResource(R.string.delete)) { dialog = "delete" }
                    }
                }
            }
            if (pl.items.isEmpty()) item("empty") { EmptyBox(stringResource(R.string.playlist_empty)) }
            itemsIndexed(pl.items, key = { _, it -> it.entryId ?: it.id }) { i, it ->
                val interaction = remember { MutableInteractionSource() }
                val shape = RoundedCornerShape(12.dp)
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = pagePadding() - 6.dp, vertical = 2.dp).focusRing(interaction, shape, grow = false).clip(shape)
                        .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple()) { nav.play(it.id, playlistId = pl.id, index = i) }
                        .padding(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("${i + 1}", style = MaterialTheme.typography.labelLarge, color = tokens.inkSubtle, modifier = Modifier.width(28.dp))
                    Box(Modifier.width(56.dp).aspectRatio(2f / 3f).clip(RoundedCornerShape(6.dp)).background(tokens.placeholder)) {
                        (it.images.poster ?: it.images.still)?.let { p -> AsyncImage(Api.image(p, 120), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()) }
                    }
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(if (it.kind == "episode") it.seriesTitle ?: it.title else it.title, style = MaterialTheme.typography.titleSmall, color = tokens.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(posterMeta(it, true), style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    if (pl.mine && it.entryId != null) {
                        if (i > 0) IconButton(onClick = { act { Api.send("/api/v1/playlists/$id/move", "POST", MoveBody(it.entryId, i - 1)) } }) {
                            Icon(Icons.Filled.KeyboardArrowUp, stringResource(R.string.move_up), tint = tokens.inkMuted)
                        }
                        if (i < pl.items.size - 1) IconButton(onClick = { act { Api.send("/api/v1/playlists/$id/move", "POST", MoveBody(it.entryId, i + 1)) } }) {
                            Icon(Icons.Filled.KeyboardArrowDown, stringResource(R.string.move_down), tint = tokens.inkMuted)
                        }
                        IconButton(onClick = { act { Api.delete("/api/v1/playlists/$id/items/${it.entryId}") } }) {
                            Icon(Icons.Filled.Close, stringResource(R.string.remove), tint = tokens.inkMuted)
                        }
                    }
                }
            }
        }
        when (dialog) {
            "rename" -> NameDialog(stringResource(R.string.rename), pl.name, onClose = { dialog = null }) { name ->
                act { Api.send("/api/v1/playlists/$id", "PATCH", PlaylistPatch(name = name)) }
            }
            "delete" -> AlertDialog(
                onDismissRequest = { dialog = null },
                containerColor = tokens.surface1,
                title = { Text(stringResource(R.string.playlist_delete_title, pl.name), color = tokens.ink) },
                confirmButton = {
                    TextButton(onClick = {
                        dialog = null
                        scope.launch { runCatching { Api.delete("/api/v1/playlists/$id") }; nav.back() }
                    }) { Text(stringResource(R.string.delete), color = tokens.danger) }
                },
                dismissButton = { TextButton(onClick = { dialog = null }) { Text(stringResource(R.string.cancel), color = tokens.inkMuted) } },
            )
        }
    }
}

@Composable
fun NameDialog(title: String, initial: String, onClose: () -> Unit, onSave: (String) -> Unit) {
    var name by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onClose,
        containerColor = tokens.surface1,
        title = { Text(title, color = tokens.ink) },
        text = { OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.name)) }, singleLine = true, colors = plateaFieldColors(), shape = app.platea.ui.theme.Radius.md, modifier = Modifier.fillMaxWidth()) },
        confirmButton = { TextButton(enabled = name.isNotBlank(), onClick = { onSave(name.trim()); onClose() }) { Text(stringResource(R.string.save), color = tokens.blueText) } },
        dismissButton = { TextButton(onClick = onClose) { Text(stringResource(R.string.cancel), color = tokens.inkMuted) } },
    )
}
