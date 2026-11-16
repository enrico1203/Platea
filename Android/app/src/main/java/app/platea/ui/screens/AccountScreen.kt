package app.platea.ui.screens

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import app.platea.BuildConfig
import app.platea.R
import app.platea.data.Api
import app.platea.data.DeviceList
import app.platea.data.Profile
import app.platea.data.Session
import app.platea.data.UserPrefs
import app.platea.ui.LocalNav
import app.platea.ui.components.LocalIsTv
import app.platea.ui.components.PrimaryButton
import app.platea.ui.components.SecondaryButton
import app.platea.ui.components.focusRing
import app.platea.ui.components.pagePadding
import app.platea.ui.components.rememberLoad
import app.platea.ui.theme.tokens
import app.platea.util.Fmt
import coil.compose.AsyncImage
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

@kotlinx.serialization.Serializable
data class PasswordBody(val current: String, val new: String)

@kotlinx.serialization.Serializable
data class CodeBody(val code: String)

/** Sezione dell'account con titolo. */
@Composable
private fun Section(title: String, content: @Composable () -> Unit) {
    Surface(color = tokens.surface1, shape = RoundedCornerShape(14.dp), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, color = tokens.ink)
            content()
        }
    }
}

@Composable
private fun SwitchRow(label: String, hint: String? = null, checked: Boolean, onChange: (Boolean) -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(10.dp)
    Row(
        Modifier.fillMaxWidth().focusRing(interaction, shape, grow = false).clip(shape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple()) { onChange(!checked) }
            .padding(vertical = 4.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodyLarge, color = tokens.ink)
            if (hint != null) Text(hint, style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
        }
        Spacer(Modifier.width(12.dp))
        Switch(
            checked = checked, onCheckedChange = null,
            colors = SwitchDefaults.colors(checkedTrackColor = tokens.blue, checkedThumbColor = androidx.compose.ui.graphics.Color.White, uncheckedTrackColor = tokens.surface3, uncheckedBorderColor = tokens.lineStrong),
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AccountScreen() {
    val me = Session.me ?: return
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val nav = LocalNav.current
    val tv = LocalIsTv.current
    val network = stringResource(R.string.error_network)

    // Le preferenze si salvano intere: si parte dall'oggetto del server, così i campi che l'app non conosce restano.
    var raw by remember { mutableStateOf<JsonObject?>(null) }
    var prefs by remember { mutableStateOf(me.preferences) }
    LaunchedEffect(Unit) { runCatching { raw = Api.get<JsonObject>("/api/v1/me/preferences") } }
    fun update(vararg patch: Pair<String, JsonElement>) {
        val base = raw ?: Api.json.encodeToJsonElement(UserPrefs.serializer(), prefs) as JsonObject
        val next = JsonObject(base + patch)
        raw = next
        prefs = Api.json.decodeFromJsonElement(UserPrefs.serializer(), next)
        Session.setPrefs(prefs)
        scope.launch {
            runCatching { Api.send("/api/v1/me/preferences", "PUT", next.toString()) }
                .onFailure { Toast.makeText(context, Api.message(it, network), Toast.LENGTH_SHORT).show() }
        }
    }
    fun s(v: String) = JsonPrimitive(v)
    fun n(v: Number) = JsonPrimitive(v)
    fun b(v: Boolean) = JsonPrimitive(v)

    val devices = rememberLoad("devices") { Api.get<DeviceList>("/api/v1/me/devices").items }
    val seconds = { x: Int -> context.getString(R.string.seconds_n, x) }
    val langs = Fmt.LANGUAGES.map { it to Fmt.language(it) }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).windowInsetsPadding(WindowInsets.navigationBars),
    ) {
        PageTitle(stringResource(R.string.nav_account))
        Column(
            Modifier.padding(horizontal = pagePadding()).padding(bottom = 32.dp).widthIn(max = 820.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // Profilo
            Surface(color = tokens.surface1, shape = RoundedCornerShape(14.dp), modifier = Modifier.fillMaxWidth()) {
                Row(Modifier.padding(18.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(56.dp).clip(CircleShape).background(tokens.blueSoft), contentAlignment = Alignment.Center) {
                        if (me.avatar != null) AsyncImage(Api.url(me.avatar), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                        else Text(me.username.take(1).uppercase(), style = MaterialTheme.typography.titleLarge, color = tokens.blueText)
                    }
                    Spacer(Modifier.width(14.dp))
                    Column(Modifier.weight(1f)) {
                        Text(me.username, style = MaterialTheme.typography.titleLarge, color = tokens.ink)
                        Text(listOfNotNull(Session.info?.name, Api.store.server).joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
                        me.permissions.expiresAt?.let {
                            Text(
                                if (me.permissions.expired) stringResource(R.string.account_expired) else stringResource(R.string.account_expires, Fmt.date(it)),
                                style = MaterialTheme.typography.bodySmall, color = if (me.permissions.expired) tokens.danger else tokens.warning,
                            )
                        }
                    }
                }
            }

            Section(stringResource(R.string.prefs_playback)) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Dropdown(stringResource(R.string.prefs_audio_language), prefs.audioLanguage, listOf("" to stringResource(R.string.prefs_file_default)) + langs) { update("audio_language" to s(it)) }
                    Dropdown(stringResource(R.string.prefs_subtitle_language), prefs.subtitleLanguage, listOf("" to stringResource(R.string.prefs_ui_language)) + langs) { update("subtitle_language" to s(it)) }
                    Dropdown(stringResource(R.string.prefs_subtitle_mode), prefs.subtitleMode, listOf(
                        "default" to stringResource(R.string.sub_mode_default), "smart" to stringResource(R.string.sub_mode_smart),
                        "always" to stringResource(R.string.sub_mode_always), "only_forced" to stringResource(R.string.sub_mode_forced),
                        "none" to stringResource(R.string.sub_mode_none),
                    )) { update("subtitle_mode" to s(it)) }
                    Dropdown(stringResource(R.string.prefs_subtitle_size), prefs.subtitleSize, listOf(
                        "s" to stringResource(R.string.size_s), "m" to stringResource(R.string.size_m), "l" to stringResource(R.string.size_l), "xl" to stringResource(R.string.size_xl),
                    )) { update("subtitle_size" to s(it)) }
                }
                SwitchRow(stringResource(R.string.prefs_subtitle_background), checked = prefs.subtitleBackground) { update("subtitle_background" to b(it)) }
                val qualities = listOf(0L, 40_000_000L, 20_000_000L, 8_000_000L, 4_000_000L, 2_000_000L, 1_000_000L).map { q ->
                    q.toString() to (if (q == 0L) context.getString(R.string.quality_original) else Fmt.bitrate(q))
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Dropdown(stringResource(R.string.prefs_quality_local), prefs.maxBitrateLocal.toString(), qualities) { update("max_bitrate_local" to n(it.toLong())) }
                    Dropdown(stringResource(R.string.prefs_quality_remote), prefs.maxBitrateRemote.toString(), qualities) { update("max_bitrate_remote" to n(it.toLong())) }
                }
                SwitchRow(stringResource(R.string.prefs_remember_tracks), stringResource(R.string.prefs_remember_tracks_hint), prefs.rememberTracks) { update("remember_tracks" to b(it)) }
                SwitchRow(stringResource(R.string.prefs_autoplay), stringResource(R.string.prefs_autoplay_hint), prefs.autoplayNext) { update("autoplay_next" to b(it)) }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (prefs.autoplayNext) {
                        Dropdown(stringResource(R.string.prefs_countdown), prefs.autoplayCountdownSeconds.toString(), listOf(0, 5, 10, 15, 20, 30).map {
                            it.toString() to (if (it == 0) context.getString(R.string.immediately) else seconds(it))
                        }) { update("autoplay_countdown_seconds" to n(it.toInt())) }
                    }
                    Dropdown(stringResource(R.string.prefs_skip_back), prefs.skipBackSeconds.toString(), listOf(5, 10, 15, 30).map { it.toString() to seconds(it) }) { update("skip_back_seconds" to n(it.toInt())) }
                    Dropdown(stringResource(R.string.prefs_skip_forward), prefs.skipForwardSeconds.toString(), listOf(10, 15, 30, 60, 90).map { it.toString() to seconds(it) }) { update("skip_forward_seconds" to n(it.toInt())) }
                }
            }

            Section(stringResource(R.string.prefs_home)) {
                val all = listOf("resume" to R.string.home_resume, "next_up" to R.string.home_next_up, "recent" to R.string.home_recent)
                all.forEach { (key, label) ->
                    SwitchRow(stringResource(label), checked = key in prefs.homeSections) { on ->
                        val list = if (on) prefs.homeSections + key else prefs.homeSections - key
                        update("home_sections" to kotlinx.serialization.json.JsonArray(list.distinct().map { JsonPrimitive(it) }))
                    }
                }
                SwitchRow(stringResource(R.string.prefs_hide_played), checked = prefs.hidePlayedInRecent) { update("hide_played_in_recent" to b(it)) }
                if (!tv) {
                    Dropdown(stringResource(R.string.prefs_theme), prefs.theme, listOf(
                        "auto" to stringResource(R.string.theme_auto), "dark" to stringResource(R.string.theme_dark), "light" to stringResource(R.string.theme_light),
                    )) { update("theme" to s(it)) }
                }
            }

            Section(stringResource(R.string.link_title)) { LinkDevice() }

            Section(stringResource(R.string.devices)) {
                (devices.data ?: emptyList()).forEach { d ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(d.deviceName + if (d.current) " · " + stringResource(R.string.this_device) else "", style = MaterialTheme.typography.bodyLarge, color = tokens.ink)
                            Text(listOfNotNull(d.client.takeIf { it.isNotBlank() }?.let { "$it ${d.clientVersion}".trim() }, if (d.lastUsedAt > 0) Fmt.dateTime(d.lastUsedAt) else null, d.lastIp).joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
                        }
                        if (!d.current && me.permissions.manageDevices) {
                            IconButton(onClick = {
                                scope.launch { runCatching { Api.delete("/api/v1/me/devices/${d.id}") }; devices.reload() }
                            }) { Icon(Icons.Filled.Close, stringResource(R.string.revoke), tint = tokens.inkMuted) }
                        }
                    }
                }
                if (me.permissions.manageDevices && (devices.data?.size ?: 0) > 1) {
                    SecondaryButton(stringResource(R.string.revoke_others)) {
                        scope.launch { runCatching { Api.send("/api/v1/me/devices/revoke-others", "POST", null) }; devices.reload() }
                    }
                }
            }

            if (!tv) Section(stringResource(R.string.password_change)) { PasswordForm() }

            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                SecondaryButton(stringResource(R.string.about), Icons.Outlined.Info) { nav.about() }
                SecondaryButton(stringResource(R.string.change_server), Icons.Filled.Dns) { Session.forgetServer() }
            }
            PrimaryButton(stringResource(R.string.logout), Icons.AutoMirrored.Filled.Logout) { scope.launch { Session.logout() } }
            Text("Platea ${BuildConfig.VERSION_NAME} · ${Api.clientName}", style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
        }
    }
}

/** Quick Connect dal lato di chi è già collegato: conferma il codice mostrato da un altro dispositivo. */
@Composable
private fun LinkDevice() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var code by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    Text(stringResource(R.string.link_hint), style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted)
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        OutlinedTextField(
            code, { v -> code = v.filter { it.isDigit() }.take(6) },
            label = { Text(stringResource(R.string.link_code)) }, singleLine = true, colors = plateaFieldColors(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
            modifier = Modifier.weight(1f),
        )
        PrimaryButton(stringResource(R.string.link_action), enabled = code.length == 6 && !busy) {
            busy = true
            scope.launch {
                runCatching { Api.send("/api/v1/quickconnect/authorize", "POST", CodeBody(code)) }
                    .onSuccess { Toast.makeText(context, R.string.link_done, Toast.LENGTH_SHORT).show(); code = "" }
                    .onFailure { Toast.makeText(context, Api.message(it, context.getString(R.string.error_network)), Toast.LENGTH_LONG).show() }
                busy = false
            }
        }
    }
}

@Composable
private fun PasswordForm() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var current by remember { mutableStateOf("") }
    var next by remember { mutableStateOf("") }
    OutlinedTextField(current, { current = it }, label = { Text(stringResource(R.string.password_current)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), colors = plateaFieldColors(), modifier = Modifier.fillMaxWidth())
    OutlinedTextField(next, { next = it }, label = { Text(stringResource(R.string.password_new)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), colors = plateaFieldColors(), modifier = Modifier.fillMaxWidth())
    PrimaryButton(stringResource(R.string.save), enabled = current.isNotEmpty() && next.length >= 4) {
        scope.launch {
            runCatching { Api.send("/api/v1/me/password", "POST", PasswordBody(current, next)) }
                .onSuccess { Toast.makeText(context, R.string.password_changed, Toast.LENGTH_SHORT).show(); current = ""; next = "" }
                .onFailure { Toast.makeText(context, Api.message(it, context.getString(R.string.error_network)), Toast.LENGTH_LONG).show() }
        }
    }
}

@Composable
fun AboutScreen() {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        PageTitle(stringResource(R.string.about), back = true)
        Column(Modifier.padding(horizontal = pagePadding()).widthIn(max = 760.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Section("Platea") {
                Text(stringResource(R.string.about_text), style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted)
                Text("App ${BuildConfig.VERSION_NAME} · Server ${Session.info?.version ?: "?"}", style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
            }
            Section(stringResource(R.string.about_device)) {
                Profile.build(LocalContext.current, null)
                (listOf(Api.deviceName, Api.clientName) + Profile.summary).forEach {
                    Text(it, style = MaterialTheme.typography.bodySmall, color = tokens.inkMuted)
                }
            }
            Section(stringResource(R.string.about_tmdb_title)) {
                Text(stringResource(R.string.about_tmdb), style = MaterialTheme.typography.bodySmall, color = tokens.inkMuted)
            }
            Section(stringResource(R.string.about_licenses)) {
                Text(stringResource(R.string.about_licenses_text), style = MaterialTheme.typography.bodySmall, color = tokens.inkMuted)
            }
            Spacer(Modifier.height(32.dp))
        }
    }
}
