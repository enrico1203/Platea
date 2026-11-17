package app.platea.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

/*
 * Modelli dell'API pubblica di Platea (`/api/v1`). Ricalcano `WebApp/src/api/types.ts`, che è la fonte degli schemi
 * del documento OpenAPI. I campi sconosciuti si ignorano: un server più nuovo non rompe l'app.
 */

@Serializable
data class SystemInfo(
    val name: String = "Platea",
    val id: String = "",
    val version: String = "",
    val product: String = "",
    @SerialName("setup_required") val setupRequired: Boolean = false,
    @SerialName("quick_connect") val quickConnect: Boolean = false,
    @SerialName("login_message") val loginMessage: String? = null,
    /** Personalizzazione: con white_label l'app mostra nome e logo del server al posto di Platea. */
    @SerialName("white_label") val whiteLabel: Boolean = false,
    val logo: String? = null,
) {
    /** Nome da mostrare al posto di "Platea" (marchio e testi). */
    val appName: String get() = if (whiteLabel) name.ifBlank { "Platea" } else "Platea"
}

@Serializable
data class UserRef(val id: String, val username: String, @SerialName("is_admin") val isAdmin: Boolean = false)

@Serializable
data class LoginResponse(val token: String, val user: UserRef? = null)

@Serializable
data class Permissions(
    @SerialName("allow_download") val allowDownload: Boolean = false,
    @SerialName("allow_transcode") val allowTranscode: Boolean = true,
    @SerialName("max_bitrate") val maxBitrate: Long? = null,
    val expired: Boolean = false,
    @SerialName("expires_at") val expiresAt: Long? = null,
    @SerialName("within_schedule") val withinSchedule: Boolean = true,
    @SerialName("manage_devices") val manageDevices: Boolean = true,
)

@Serializable
data class UserPrefs(
    @SerialName("audio_language") val audioLanguage: String = "",
    @SerialName("subtitle_language") val subtitleLanguage: String = "",
    @SerialName("subtitle_mode") val subtitleMode: String = "default",
    @SerialName("autoplay_next") val autoplayNext: Boolean = true,
    @SerialName("autoplay_countdown_seconds") val autoplayCountdownSeconds: Int = 10,
    @SerialName("skip_back_seconds") val skipBackSeconds: Int = 10,
    @SerialName("skip_forward_seconds") val skipForwardSeconds: Int = 30,
    @SerialName("subtitle_size") val subtitleSize: String = "m",
    @SerialName("subtitle_background") val subtitleBackground: Boolean = false,
    @SerialName("home_sections") val homeSections: List<String> = listOf("resume", "next_up", "recent"),
    @SerialName("home_hidden_libraries") val homeHiddenLibraries: List<String> = emptyList(),
    @SerialName("hide_played_in_recent") val hidePlayedInRecent: Boolean = false,
    val theme: String = "auto",
    @SerialName("remember_tracks") val rememberTracks: Boolean = true,
    @SerialName("max_bitrate_local") val maxBitrateLocal: Long = 0,
    @SerialName("max_bitrate_remote") val maxBitrateRemote: Long = 0,
)

@Serializable
data class Me(
    val id: String,
    val username: String,
    @SerialName("is_admin") val isAdmin: Boolean = false,
    @SerialName("device_name") val deviceName: String = "",
    val permissions: Permissions = Permissions(),
    val avatar: String? = null,
    val preferences: UserPrefs = UserPrefs(),
)

@Serializable
data class UserData(
    @SerialName("position_ms") val positionMs: Long = 0,
    val played: Boolean = false,
    @SerialName("play_count") val playCount: Int = 0,
    val favorite: Boolean = false,
)

@Serializable
data class Images(
    val poster: String? = null,
    @SerialName("poster_color") val posterColor: String? = null,
    val backdrop: String? = null,
    val still: String? = null,
    val logo: String? = null,
)

@Serializable
data class Item(
    val id: String,
    @SerialName("library_id") val libraryId: String = "",
    val kind: String = "movie",
    val title: String = "",
    @SerialName("original_title") val originalTitle: String? = null,
    val year: Int? = null,
    @SerialName("index_number") val indexNumber: Int? = null,
    @SerialName("index_end") val indexEnd: Int? = null,
    @SerialName("parent_index_number") val parentIndexNumber: Int? = null,
    @SerialName("parent_id") val parentId: String? = null,
    @SerialName("series_id") val seriesId: String? = null,
    @SerialName("series_title") val seriesTitle: String? = null,
    val overview: String? = null,
    @SerialName("runtime_ms") val runtimeMs: Long? = null,
    @SerialName("community_rating") val communityRating: Double? = null,
    @SerialName("official_rating") val officialRating: String? = null,
    @SerialName("premiere_date") val premiereDate: String? = null,
    val available: Boolean = true,
    @SerialName("match_state") val matchState: String = "",
    @SerialName("added_at") val addedAt: Long = 0,
    val images: Images = Images(),
    @SerialName("user_data") val userData: UserData = UserData(),
    @SerialName("child_count") val childCount: Int? = null,
    @SerialName("unplayed_count") val unplayedCount: Int? = null,
    // Solo nelle playlist e nella pagina della persona.
    @SerialName("entry_id") val entryId: String? = null,
    val roles: List<PersonRole> = emptyList(),
) {
    val isPlayable get() = kind == "movie" || kind == "episode"
    val episodeCode get() = "S${parentIndexNumber ?: 0} · E${indexNumber ?: 0}"
    val progress: Float
        get() {
            val r = runtimeMs ?: return 0f
            if (r <= 0 || userData.positionMs <= 0 || userData.played) return 0f
            return (userData.positionMs.toFloat() / r).coerceIn(0f, 1f)
        }
}

@Serializable
data class PersonRole(val role: String = "", val character: String? = null)

@Serializable
data class VideoStream(
    val index: Int = 0,
    val codec: String = "",
    val profile: String? = null,
    @SerialName("bit_depth") val bitDepth: Int? = null,
    val width: Int? = null,
    val height: Int? = null,
    @SerialName("frame_rate") val frameRate: Double? = null,
    val hdr: String? = null,
    val bitrate: Long? = null,
)

@Serializable
data class AudioStream(
    val index: Int = 0,
    val codec: String = "",
    val channels: Int? = null,
    @SerialName("channel_layout") val channelLayout: String? = null,
    val language: String? = null,
    val title: String? = null,
    val default: Boolean = false,
)

@Serializable
data class SubtitleStream(
    val index: Int = 0,
    val codec: String = "",
    val language: String? = null,
    val title: String? = null,
    val default: Boolean = false,
    val forced: Boolean = false,
    val external: Boolean = false,
    val image: Boolean = false,
)

@Serializable
data class MediaFile(
    val id: String,
    val name: String = "",
    @SerialName("version_label") val versionLabel: String? = null,
    val container: String? = null,
    val size: Long = 0,
    @SerialName("duration_ms") val durationMs: Long? = null,
    val bitrate: Long? = null,
    val video: List<VideoStream> = emptyList(),
    val audio: List<AudioStream> = emptyList(),
    val subtitles: List<SubtitleStream> = emptyList(),
)

@Serializable
data class Person(val id: String, val name: String, val role: String = "", val character: String? = null, val image: String? = null)

@Serializable
data class Trailer(val key: String, val name: String? = null, val lang: String? = null)

@Serializable
data class Extra(@SerialName("file_id") val fileId: String, val title: String = "", val kind: String? = null, @SerialName("duration_ms") val durationMs: Long? = null)

@Serializable
data class Ref(val id: String, val title: String = "")

@Serializable
data class ItemDetail(
    val id: String,
    @SerialName("library_id") val libraryId: String = "",
    val kind: String = "movie",
    val title: String = "",
    @SerialName("original_title") val originalTitle: String? = null,
    val year: Int? = null,
    @SerialName("index_number") val indexNumber: Int? = null,
    @SerialName("parent_index_number") val parentIndexNumber: Int? = null,
    @SerialName("parent_id") val parentId: String? = null,
    @SerialName("series_id") val seriesId: String? = null,
    @SerialName("series_title") val seriesTitle: String? = null,
    val overview: String? = null,
    val tagline: String? = null,
    @SerialName("runtime_ms") val runtimeMs: Long? = null,
    @SerialName("community_rating") val communityRating: Double? = null,
    @SerialName("official_rating") val officialRating: String? = null,
    @SerialName("premiere_date") val premiereDate: String? = null,
    val available: Boolean = true,
    @SerialName("match_state") val matchState: String = "",
    val images: Images = Images(),
    @SerialName("user_data") val userData: UserData = UserData(),
    @SerialName("child_count") val childCount: Int? = null,
    val trailers: List<Trailer> = emptyList(),
    val extras: List<Extra> = emptyList(),
    val genres: List<String> = emptyList(),
    val studios: List<String> = emptyList(),
    val people: List<Person> = emptyList(),
    val files: List<MediaFile> = emptyList(),
    val collections: List<Ref> = emptyList(),
) {
    val isPlayable get() = kind == "movie" || kind == "episode"
}

@Serializable
data class ItemPage(val items: List<Item> = emptyList(), @SerialName("next_cursor") val nextCursor: String? = null)

@Serializable
data class Library(
    val id: String,
    val name: String = "",
    val kind: String = "movies",
    @SerialName("item_count") val itemCount: Int = 0,
)

@Serializable
data class LibraryPage(val items: List<Library> = emptyList())

@Serializable
data class MissingEpisode(val season: Int = 0, val episode: Int = 0, val title: String? = null, val overview: String? = null, @SerialName("air_date") val airDate: String? = null)

@Serializable
data class MissingList(val items: List<MissingEpisode> = emptyList())

@Serializable
data class RecentRow(
    @SerialName("library_id") val libraryId: String,
    @SerialName("library_name") val libraryName: String = "",
    val kind: String = "movies",
    val items: List<Item> = emptyList(),
)

@Serializable
data class Home(
    val resume: List<Item> = emptyList(),
    @SerialName("next_up") val nextUp: List<Item> = emptyList(),
    val recent: List<RecentRow> = emptyList(),
)

@Serializable
data class Genre(val name: String, val count: Int = 0)

@Serializable
data class GenreList(val items: List<Genre> = emptyList())

@Serializable
data class Chapter(@SerialName("start_ms") val startMs: Long, val title: String? = null)

@Serializable
data class ChapterList(val items: List<Chapter> = emptyList())

@Serializable
data class NextItem(val item: Item? = null)

@Serializable
data class PersonDetail(val id: String, val name: String = "", val image: String? = null, val items: List<Item> = emptyList())

// --- Riproduzione ---

@Serializable
data class VideoCodecSupport(
    val codec: String,
    val profiles: List<String> = emptyList(),
    @SerialName("max_level") val maxLevel: Int? = null,
    @SerialName("max_bit_depth") val maxBitDepth: Int? = null,
    @SerialName("max_width") val maxWidth: Int? = null,
    @SerialName("max_height") val maxHeight: Int? = null,
)

@Serializable
data class AudioCodecSupport(val codec: String, @SerialName("max_channels") val maxChannels: Int? = null)

@Serializable
data class DeviceProfile(
    val name: String,
    val containers: List<String>,
    @SerialName("video_codecs") val videoCodecs: List<VideoCodecSupport>,
    @SerialName("audio_codecs") val audioCodecs: List<AudioCodecSupport>,
    @SerialName("subtitle_text") val subtitleText: List<String>,
    @SerialName("subtitle_image") val subtitleImage: List<String>,
    val hdr: List<String>,
    @SerialName("max_bitrate") val maxBitrate: Long?,
    val protocols: List<String>,
    @SerialName("hls_video_codecs") val hlsVideoCodecs: List<String>,
    @SerialName("hls_audio_codecs") val hlsAudioCodecs: List<String>,
)

@Serializable
data class PlaybackInfoRequest(
    @SerialName("item_id") val itemId: String,
    @SerialName("file_id") val fileId: String? = null,
    val profile: DeviceProfile,
    @SerialName("audio_index") val audioIndex: Int? = null,
    @SerialName("subtitle_index") val subtitleIndex: Int? = null,
    @SerialName("start_ms") val startMs: Long = 0,
    @SerialName("max_bitrate") val maxBitrate: Long? = null,
    @SerialName("force_transcode") val forceTranscode: Boolean = false,
    @SerialName("ui_language") val uiLanguage: String? = null,
    @SerialName("explicit_quality") val explicitQuality: Boolean = false,
)

@Serializable
data class AudioTrack(val index: Int, val codec: String = "", val channels: Int = 2, val language: String? = null, val default: Boolean = false)

@Serializable
data class SubtitleTrack(
    val index: Int,
    val codec: String = "",
    val language: String? = null,
    val title: String? = null,
    val external: Boolean = false,
    val forced: Boolean = false,
    val default: Boolean = false,
    val image: Boolean = false,
    val url: String? = null,
)

@Serializable
data class SubtitleDelivery(val delivery: String = "none", val index: Int? = null)

@Serializable
data class Trickplay(
    @SerialName("interval_ms") val intervalMs: Long,
    val width: Int,
    val height: Int,
    val cols: Int,
    val rows: Int,
    val count: Int,
    val url: String,
)

@Serializable
data class PlaybackInfo(
    @SerialName("session_id") val sessionId: String,
    val method: String = "direct_play",
    val reasons: List<String> = emptyList(),
    val chain: String = "",
    @SerialName("stream_url") val streamUrl: String? = null,
    @SerialName("hls_url") val hlsUrl: String? = null,
    @SerialName("file_id") val fileId: String = "",
    @SerialName("duration_ms") val durationMs: Long = 0,
    @SerialName("start_ms") val startMs: Long = 0,
    @SerialName("audio_index") val audioIndex: Int? = null,
    @SerialName("audio_tracks") val audioTracks: List<AudioTrack> = emptyList(),
    val subtitles: List<SubtitleTrack> = emptyList(),
    val subtitle: SubtitleDelivery = SubtitleDelivery(),
    @SerialName("subtitle_default") val subtitleDefault: Int? = null,
    val trickplay: Trickplay? = null,
    @SerialName("max_bitrate") val maxBitrate: Long? = null,
    val container: String = "",
)

@Serializable
data class ProgressRequest(@SerialName("position_ms") val positionMs: Long, val paused: Boolean = false)

@Serializable
data class TracksRequest(
    @SerialName("audio_index") val audioIndex: Int? = null,
    @SerialName("subtitle_index") val subtitleIndex: Int? = null,
    @SerialName("subtitle_off") val subtitleOff: Boolean = false,
)

// --- Playlist ---

@Serializable
data class PlaylistSummary(
    val id: String,
    val name: String = "",
    val count: Int = 0,
    @SerialName("runtime_ms") val runtimeMs: Long? = null,
    val poster: String? = null,
    val shared: Boolean = false,
    val mine: Boolean = true,
    val owner: String? = null,
)

@Serializable
data class PlaylistList(val items: List<PlaylistSummary> = emptyList())

@Serializable
data class PlaylistDetail(
    val id: String,
    val name: String = "",
    @SerialName("runtime_ms") val runtimeMs: Long = 0,
    val items: List<Item> = emptyList(),
    val shared: Boolean = false,
    val mine: Boolean = true,
    val owner: String? = null,
)

// --- Account ---

@Serializable
data class Device(
    val id: String,
    @SerialName("device_name") val deviceName: String = "",
    val client: String = "",
    @SerialName("client_version") val clientVersion: String = "",
    @SerialName("last_used_at") val lastUsedAt: Long = 0,
    @SerialName("last_ip") val lastIp: String? = null,
    val current: Boolean = false,
)

@Serializable
data class DeviceList(val items: List<Device> = emptyList())

@Serializable
data class QuickConnectInitiate(val code: String, val secret: String, @SerialName("expires_at") val expiresAt: Long = 0)

@Serializable
data class QuickConnectState(val state: String = "pending")

@Serializable
data class DownloadLink(val url: String, val name: String = "", val size: Long = 0)

@Serializable
data class ServerEvent(val type: String, val data: JsonObject? = null)

@Serializable
data class ApiErrorBody(val error: String? = null, val message: String? = null)
