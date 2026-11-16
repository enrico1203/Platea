package app.platea

import app.platea.data.Api
import app.platea.data.ApiErrorBody
import app.platea.data.AudioCodecSupport
import app.platea.data.ChapterList
import app.platea.data.DeviceList
import app.platea.data.DeviceProfile
import app.platea.data.GenreList
import app.platea.data.Home
import app.platea.data.ItemDetail
import app.platea.data.ItemPage
import app.platea.data.Library
import app.platea.data.LibraryPage
import app.platea.data.LoginResponse
import app.platea.data.Me
import app.platea.data.MissingList
import app.platea.data.NextItem
import app.platea.data.PlaybackInfo
import app.platea.data.PlaybackInfoRequest
import app.platea.data.PlaylistDetail
import app.platea.data.PlaylistList
import app.platea.data.QuickConnectInitiate
import app.platea.data.QuickConnectState
import app.platea.data.SystemInfo
import app.platea.data.UserPrefs
import app.platea.data.VideoCodecSupport
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Le risposte vere del server (salvate da `scripts/android-test/contract.py`) si leggono con i modelli dell'app.
 * Se il server cambia forma a un campo che l'app usa, questo test se ne accorge.
 */
class ContractTest {
    private val dir = File(javaClass.classLoader!!.getResource("contract")!!.toURI())

    private fun <T> read(name: String, ser: KSerializer<T>): T =
        Api.json.decodeFromString(ser, File(dir, "$name.json").readText())

    private fun names(prefix: String) = dir.listFiles()!!.map { it.nameWithoutExtension }.filter { it.startsWith(prefix) }.sorted()

    @Test
    fun session() {
        val info = read("system_info", SystemInfo.serializer())
        assertFalse(info.setupRequired)
        assertTrue(info.version.isNotEmpty())
        val me = read("me", Me.serializer())
        assertEquals("admin", me.username)
        assertTrue(me.isAdmin)
        read("preferences", UserPrefs.serializer())
        assertTrue(read("devices", DeviceList.serializer()).items.isNotEmpty())
        assertNotNull(read("login", LoginResponse.serializer()).token)
        assertEquals("<token>", read("qc_redeem", LoginResponse.serializer()).token)
        assertEquals(6, read("qc_initiate", QuickConnectInitiate.serializer()).code.length)
        read("qc_status", QuickConnectState.serializer())
        assertTrue(read("error_login", ApiErrorBody.serializer()).message!!.isNotEmpty())
    }

    @Test
    fun catalog() {
        assertEquals(2, read("libraries", LibraryPage.serializer()).items.size)
        assertEquals("movies", read("library", Library.serializer()).kind)
        read("genres", GenreList.serializer())
        for (n in listOf("items_movies", "items_shows", "items_letter", "search", "seasons", "episodes") + names("similar_")) {
            read(n, ItemPage.serializer())
        }
        val movies = read("items_movies", ItemPage.serializer()).items
        assertEquals(3, movies.size)
        assertTrue(movies.all { it.kind == "movie" && it.title.isNotEmpty() })
        val eps = read("episodes", ItemPage.serializer()).items
        assertTrue(eps.all { it.kind == "episode" && it.indexNumber != null && it.parentIndexNumber != null })
        assertEquals("S1 · E1", eps[0].episodeCode)
        read("missing", MissingList.serializer())
        assertNotNull(read("next", NextItem.serializer()).item)
        for (n in names("chapters_")) read(n, ChapterList.serializer())
        val home = read("home", Home.serializer())
        assertTrue(home.recent.isNotEmpty())
    }

    @Test
    fun details() {
        val series = read("series", ItemDetail.serializer())
        assertEquals("series", series.kind)
        val ep = read("episode", ItemDetail.serializer())
        assertTrue(ep.isPlayable)
        assertNotNull(ep.seriesId)
        for (n in names("movie_")) {
            val m = read(n, ItemDetail.serializer())
            assertTrue(n, m.files.isNotEmpty())
            assertTrue(n, m.files[0].video.isNotEmpty())
        }
        val after = read("movie_after", ItemDetail.serializer())
        assertTrue(after.userData.played && after.userData.favorite)
    }

    @Test
    fun playback() {
        val all = names("playback_")
        assertTrue(all.size >= 7)
        for (n in all) {
            val p = read(n, PlaybackInfo.serializer())
            assertTrue(n, p.streamUrl != null || p.hlsUrl != null)
            assertTrue(n, p.durationMs > 0)
            assertTrue(n, p.method in setOf("direct_play", "remux", "transcode"))
            if (p.method == "direct_play") assertNotNull(n, p.streamUrl) else assertNotNull(n, p.hlsUrl)
        }
        val ep = read("playback_episode_phone", PlaybackInfo.serializer())
        assertTrue(ep.subtitles.any { it.url != null && !it.image })
    }

    @Test
    fun playlists() {
        val pl = read("playlist", PlaylistDetail.serializer())
        assertEquals(3, pl.items.size)
        assertTrue(pl.items.all { it.entryId != null })
        read("playlists", PlaylistList.serializer())
    }

    /** La richiesta di riproduzione ha i nomi dei campi che il server si aspetta (serde, snake_case). */
    @Test
    fun requestShape() {
        val profile = DeviceProfile(
            "Platea Android", listOf("mkv"), listOf(VideoCodecSupport("h264", maxBitDepth = 8)), listOf(AudioCodecSupport("aac", 8)),
            listOf("vtt"), emptyList(), emptyList(), null, listOf("file", "hls"), listOf("h264"), listOf("aac"),
        )
        val body = Api.json.encodeToJsonElement(PlaybackInfoRequest.serializer(), PlaybackInfoRequest(itemId = "x", profile = profile, startMs = 5)).jsonObject
        for (k in listOf("item_id", "profile", "start_ms", "force_transcode", "explicit_quality")) assertTrue(k, k in body)
        val p = body["profile"] as JsonObject
        for (k in listOf("video_codecs", "audio_codecs", "subtitle_text", "hls_video_codecs", "hls_audio_codecs", "protocols")) assertTrue(k, k in p)
        assertTrue("max_bit_depth" in (p["video_codecs"] as kotlinx.serialization.json.JsonArray)[0].jsonObject)
    }
}
