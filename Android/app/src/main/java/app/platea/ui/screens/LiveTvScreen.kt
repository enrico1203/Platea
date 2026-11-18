package app.platea.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.platea.R
import app.platea.data.Api
import app.platea.data.LiveChannel
import app.platea.data.LiveChannelPage
import app.platea.player.LiveTvActivity
import app.platea.ui.components.EmptyBox
import app.platea.ui.components.ResContent
import app.platea.ui.components.focusRing
import app.platea.ui.components.pagePadding
import app.platea.ui.components.rememberLoad
import app.platea.ui.theme.tokens
import coil.compose.AsyncImage
import java.text.DateFormat
import java.util.Date

/** Logo del canale, o il suo numero su un fondo neutro. */
@Composable
fun ChannelLogo(c: LiveChannel, size: Int = 48) {
    Box(
        Modifier.size(size.dp).clip(RoundedCornerShape(6.dp)).background(tokens.surface2),
        contentAlignment = Alignment.Center,
    ) {
        if (c.logo != null) {
            AsyncImage(Api.image(c.logo, size * 3), null, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize().padding(2.dp))
        } else {
            Text(c.numberText.ifEmpty { c.name.take(2) }, style = MaterialTheme.typography.labelLarge, color = tokens.inkMuted)
        }
    }
}

/** Live TV: canali in ordine di numero, con il programma in onda e il successivo. Toccando si guarda. */
@Composable
fun LiveTvScreen() {
    val channels = rememberLoad("livetv") { Api.get<LiveChannelPage>("/api/v1/livetv/channels").items }
    val context = LocalContext.current
    val time = remember { DateFormat.getTimeInstance(DateFormat.SHORT) }
    Column(Modifier.fillMaxSize()) {
        val count = channels.data?.size ?: 0
        PageTitle(stringResource(R.string.tv_title), sub = if (count > 0) pluralStringResource(R.plurals.tv_channels, count, count) else null, back = true)
        ResContent(channels) { list ->
            if (list.isEmpty()) {
                EmptyBox(stringResource(R.string.tv_empty))
                return@ResContent
            }
            val ids = list.map { it.id }
            LazyColumn(contentPadding = PaddingValues(horizontal = pagePadding(), vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                items(list, key = { it.id }) { c ->
                    val interaction = remember { MutableInteractionSource() }
                    val shape = RoundedCornerShape(10.dp)
                    Row(
                        Modifier.fillMaxWidth().focusRing(interaction, shape, grow = false).clip(shape)
                            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple()) { LiveTvActivity.start(context, c.id, ids) }
                            .padding(10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        ChannelLogo(c)
                        Column(Modifier.weight(1f).padding(start = 12.dp)) {
                            Text("${c.numberText} ${c.name}".trim(), style = MaterialTheme.typography.titleSmall, color = tokens.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            val now = c.now
                            Text(now?.title ?: stringResource(R.string.tv_no_guide), style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            if (now != null && now.endMs > now.startMs) {
                                val pct = ((System.currentTimeMillis() - now.startMs).toFloat() / (now.endMs - now.startMs)).coerceIn(0f, 1f)
                                Box(Modifier.padding(top = 4.dp).fillMaxWidth(0.6f).height(3.dp).clip(RoundedCornerShape(2.dp)).background(tokens.lineStrong)) {
                                    Box(Modifier.fillMaxWidth(pct).height(3.dp).background(tokens.blue))
                                }
                            }
                            c.next?.let { n ->
                                Text(stringResource(R.string.tv_next, time.format(Date(n.startMs)), n.title), style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                }
            }
        }
    }
}
