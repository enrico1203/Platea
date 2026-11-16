package app.platea.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.platea.BuildConfig
import app.platea.R
import app.platea.data.Api
import app.platea.data.ApiException
import app.platea.data.QuickConnectBody
import app.platea.data.QuickConnectInitiate
import app.platea.data.QuickConnectState
import app.platea.data.Session
import app.platea.data.SystemInfo
import app.platea.ui.components.ErrorBox
import app.platea.ui.components.LocalIsTv
import app.platea.ui.components.Mark
import app.platea.ui.components.PrimaryButton
import app.platea.ui.components.SecondaryButton
import app.platea.ui.theme.tokens
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun LoadingScreen() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Mark(64.dp)
            Spacer(Modifier.height(24.dp))
            CircularProgressIndicator(color = tokens.blueText, strokeWidth = 3.dp)
        }
    }
}

/** Scheda centrata delle schermate di accesso. */
@Composable
private fun AuthCard(content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    Box(
        Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).imePadding().verticalScroll(rememberScrollState()),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            color = tokens.surface1,
            shape = RoundedCornerShape(14.dp),
            modifier = Modifier.padding(16.dp).widthIn(max = 460.dp).fillMaxWidth(),
        ) {
            Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp), content = content)
        }
    }
}

@Composable
private fun Brand(name: String?, sub: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Mark(44.dp)
        Spacer(Modifier.width(12.dp))
        Column {
            Text(name?.takeIf { it.isNotBlank() } ?: "Platea", style = MaterialTheme.typography.headlineSmall, color = tokens.ink)
            Text(sub, style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
        }
    }
}

@Composable
fun plateaFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = tokens.blue, unfocusedBorderColor = tokens.lineStrong, cursorColor = tokens.blueText,
    focusedLabelColor = tokens.blueText, unfocusedLabelColor = tokens.inkMuted,
    focusedTextColor = tokens.ink, unfocusedTextColor = tokens.ink,
)

@Composable
fun ConnectScreen() {
    val scope = rememberCoroutineScope()
    var address by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val unreachable = stringResource(R.string.connect_failed)
    val recent = remember { Api.store.recentServers }
    val focus = remember { FocusRequester() }
    val tv = LocalIsTv.current
    fun go(a: String) {
        if (a.isBlank() || busy) return
        busy = true
        error = null
        scope.launch {
            Session.connect(a).onFailure { error = unreachable }
            busy = false
        }
    }
    LaunchedEffect(Unit) { if (!tv) runCatching { focus.requestFocus() } }
    AuthCard {
        Brand(null, stringResource(R.string.tagline))
        Text(stringResource(R.string.connect_title), style = MaterialTheme.typography.titleMedium, color = tokens.ink)
        Text(stringResource(R.string.connect_hint), style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted)
        OutlinedTextField(
            value = address,
            onValueChange = { address = it },
            label = { Text(stringResource(R.string.connect_address)) },
            placeholder = { Text("192.168.1.10:8096") },
            singleLine = true,
            colors = plateaFieldColors(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
            keyboardActions = KeyboardActions(onGo = { go(address) }),
            modifier = Modifier.fillMaxWidth().focusRequester(focus),
        )
        error?.let { Text(it, color = tokens.danger, style = MaterialTheme.typography.bodyMedium) }
        PrimaryButton(stringResource(R.string.connect_action), enabled = !busy && address.isNotBlank(), modifier = Modifier.fillMaxWidth()) { go(address) }
        if (recent.isNotEmpty()) {
            Text(stringResource(R.string.connect_recent), style = MaterialTheme.typography.labelMedium, color = tokens.inkSubtle)
            recent.forEach { r -> SecondaryButton(r, modifier = Modifier.fillMaxWidth()) { go(r) } }
        }
    }
}

@Composable
fun UnreachableScreen(server: String) {
    val scope = rememberCoroutineScope()
    AuthCard {
        Brand(null, server)
        Text(stringResource(R.string.unreachable_title), style = MaterialTheme.typography.titleMedium, color = tokens.ink)
        Text(stringResource(R.string.unreachable_hint), style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted)
        PrimaryButton(stringResource(R.string.retry), modifier = Modifier.fillMaxWidth()) { scope.launch { Session.reload() } }
        SecondaryButton(stringResource(R.string.change_server), modifier = Modifier.fillMaxWidth()) { Session.forgetServer() }
    }
}

@Composable
fun LoginScreen(info: SystemInfo) {
    val scope = rememberCoroutineScope()
    val tv = LocalIsTv.current
    // Sulla TV si parte dal codice: scrivere la password col telecomando è scomodo.
    var quick by remember { mutableStateOf(tv && info.quickConnect) }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val network = stringResource(R.string.error_network)
    val passwordFocus = remember { FocusRequester() }

    if (info.setupRequired) {
        AuthCard {
            Brand(info.name, Api.store.server)
            Text(stringResource(R.string.setup_required), style = MaterialTheme.typography.bodyLarge, color = tokens.inkMuted)
            PrimaryButton(stringResource(R.string.retry), modifier = Modifier.fillMaxWidth()) { scope.launch { Session.reload() } }
            SecondaryButton(stringResource(R.string.change_server), modifier = Modifier.fillMaxWidth()) { Session.forgetServer() }
        }
        return
    }

    if (quick) {
        AuthCard {
            Brand(info.name, Api.store.server)
            QuickConnectLogin(onCancel = { quick = false })
        }
        return
    }

    fun submit() {
        if (busy || username.isBlank() || password.isEmpty()) return
        busy = true
        error = null
        scope.launch {
            try {
                Session.login(username.trim(), password)
            } catch (e: Exception) {
                error = if (e is ApiException && e.status != 0) Api.message(e, network) else network
            }
            busy = false
        }
    }

    AuthCard {
        Brand(info.name, Api.store.server)
        info.loginMessage?.takeIf { it.isNotBlank() }?.let {
            Surface(color = tokens.surface2, shape = RoundedCornerShape(10.dp)) {
                Text(it, color = tokens.inkMuted, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(12.dp))
            }
        }
        OutlinedTextField(
            value = username,
            onValueChange = { username = it },
            label = { Text(stringResource(R.string.username)) },
            singleLine = true,
            colors = plateaFieldColors(),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next, autoCorrectEnabled = false),
            keyboardActions = KeyboardActions(onNext = { passwordFocus.requestFocus() }),
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = password,
            onValueChange = { password = it },
            label = { Text(stringResource(R.string.password)) },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            colors = plateaFieldColors(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { submit() }),
            modifier = Modifier.fillMaxWidth().focusRequester(passwordFocus),
        )
        error?.let { Text(it, color = tokens.danger, style = MaterialTheme.typography.bodyMedium) }
        PrimaryButton(stringResource(R.string.login), enabled = !busy && username.isNotBlank() && password.isNotEmpty(), modifier = Modifier.fillMaxWidth()) { submit() }
        if (info.quickConnect) {
            SecondaryButton(stringResource(R.string.qc_login_with_code), modifier = Modifier.fillMaxWidth()) { quick = true }
        }
        TextButton(onClick = { Session.forgetServer() }, modifier = Modifier.align(Alignment.CenterHorizontally)) {
            Text(stringResource(R.string.change_server), color = tokens.inkMuted)
        }
    }
}

/**
 * Quick Connect: il dispositivo mostra un codice di 6 cifre, un altro dispositivo già collegato lo conferma
 * (web UI: Account › Collega un dispositivo, o `<server>/link`), poi il segreto diventa un token.
 */
@Composable
private fun QuickConnectLogin(onCancel: () -> Unit) {
    var req by remember { mutableStateOf<QuickConnectInitiate?>(null) }
    var expired by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var attempt by remember { mutableStateOf(0) }
    val network = stringResource(R.string.error_network)

    LaunchedEffect(attempt) {
        req = null; expired = false; error = null
        try {
            req = Api.post<QuickConnectInitiate>(
                "/api/v1/quickconnect/initiate",
                QuickConnectBody(Api.store.deviceId, Api.deviceName, Api.clientName, BuildConfig.VERSION_NAME),
            )
        } catch (e: Exception) {
            error = Api.message(e, network)
        }
    }
    LaunchedEffect(req) {
        val r = req ?: return@LaunchedEffect
        while (true) {
            delay(2000)
            try {
                val s = Api.get<QuickConnectState>("/api/v1/quickconnect/status/${r.secret}")
                if (s.state == "authorized") {
                    val t = Api.post<app.platea.data.LoginResponse>("/api/v1/quickconnect/redeem", SecretBody(r.secret))
                    Session.acceptToken(t.token)
                    return@LaunchedEffect
                }
            } catch (e: ApiException) {
                if (e.status == 404) { expired = true; return@LaunchedEffect }
            }
        }
    }

    Text(stringResource(R.string.qc_title), style = MaterialTheme.typography.titleMedium, color = tokens.ink)
    error?.let { ErrorBox(it) }
    if (expired) {
        Text(stringResource(R.string.qc_expired), color = tokens.warning, style = MaterialTheme.typography.bodyMedium)
        PrimaryButton(stringResource(R.string.qc_new_code), modifier = Modifier.fillMaxWidth()) { attempt++ }
    } else {
        val code = req?.code
        Text(
            if (code != null) "${code.take(3)} ${code.drop(3)}" else "··· ···",
            fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold, fontSize = 44.sp, letterSpacing = 4.sp,
            color = tokens.ink, textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth().background(tokens.surface2, RoundedCornerShape(12.dp)).padding(vertical = 18.dp),
        )
        Text(stringResource(R.string.qc_hint, Api.store.server + "/link"), style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted)
    }
    SecondaryButton(stringResource(R.string.qc_use_password), modifier = Modifier.fillMaxWidth()) { onCancel() }
}

@kotlinx.serialization.Serializable
data class SecretBody(val secret: String)
