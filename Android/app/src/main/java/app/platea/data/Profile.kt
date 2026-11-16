package app.platea.data

import android.content.Context
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.os.Build
import android.view.Display
import android.view.WindowManager
import androidx.media3.common.C
import androidx.media3.exoplayer.audio.AudioCapabilities

/**
 * Profilo del dispositivo: l'app dichiara ciò che sa riprodurre e il server decide (direct play, remux, transcodifica).
 * ExoPlayer legge MKV, MP4, WebM e TS da sé; i codec sono quelli dei decoder del telefono o del televisore
 * (`MediaCodecList`), più l'audio che il televisore o l'amplificatore ricevono in passthrough (AC3, E-AC3, DTS, TrueHD).
 */
object Profile {
    private var cached: DeviceProfile? = null
    /** Descrizione leggibile per la pagina Informazioni. */
    var summary: List<String> = emptyList()
        private set

    fun build(context: Context, maxBitrate: Long?): DeviceProfile {
        val p = cached ?: detect(context).also { cached = it }
        return p.copy(maxBitrate = maxBitrate)
    }

    private class Dec(val mime: String, val caps: MediaCodecInfo.CodecCapabilities)

    private fun decoders(): List<Dec> {
        val list = runCatching { MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos }.getOrDefault(emptyArray())
        val out = mutableListOf<Dec>()
        for (info in list) {
            if (info.isEncoder) continue
            for (type in info.supportedTypes) {
                val caps = runCatching { info.getCapabilitiesForType(type) }.getOrNull() ?: continue
                out += Dec(type.lowercase(), caps)
            }
        }
        return out
    }

    private fun detect(context: Context): DeviceProfile {
        val decs = decoders()
        fun all(mime: String) = decs.filter { it.mime == mime }
        fun has(mime: String) = decs.any { it.mime == mime }
        fun maxSize(mime: String): Pair<Int, Int>? {
            var w = 0
            var h = 0
            for (d in all(mime)) {
                val v = d.caps.videoCapabilities ?: continue
                w = maxOf(w, v.supportedWidths.upper)
                h = maxOf(h, v.supportedHeights.upper)
            }
            return if (w > 0 && h > 0) w to h else null
        }
        fun hasProfile(mime: String, profile: Int) = all(mime).any { d -> d.caps.profileLevels.any { it.profile == profile } }

        val video = mutableListOf<VideoCodecSupport>()
        val avcSize = maxSize("video/avc")
        video += VideoCodecSupport(
            codec = "h264",
            profiles = listOf("baseline", "constrained baseline", "main", "high"),
            maxBitDepth = 8,
            maxWidth = avcSize?.first,
            maxHeight = avcSize?.second,
        )
        val hevc = has("video/hevc")
        val hevc10 = hevc && (hasProfile("video/hevc", MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10) ||
            hasProfile("video/hevc", MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10))
        if (hevc) {
            val s = maxSize("video/hevc")
            video += VideoCodecSupport("hevc", if (hevc10) listOf("main", "main 10") else listOf("main"), null, if (hevc10) 10 else 8, s?.first, s?.second)
        }
        val av1 = has("video/av01")
        if (av1) {
            val s = maxSize("video/av01")
            video += VideoCodecSupport("av1", emptyList(), null, 10, s?.first, s?.second)
        }
        if (has("video/x-vnd.on2.vp9")) {
            val s = maxSize("video/x-vnd.on2.vp9")
            val vp9hi = hasProfile("video/x-vnd.on2.vp9", MediaCodecInfo.CodecProfileLevel.VP9Profile2)
            video += VideoCodecSupport("vp9", emptyList(), null, if (vp9hi) 10 else 8, s?.first, s?.second)
        }
        if (has("video/x-vnd.on2.vp8")) video += VideoCodecSupport("vp8")
        if (has("video/mpeg2")) video += VideoCodecSupport("mpeg2video", maxBitDepth = 8)

        // Audio: decoder del sistema più passthrough verso televisore o amplificatore.
        @Suppress("DEPRECATION")
        val pass = runCatching { AudioCapabilities.getCapabilities(context) }.getOrNull()
        @Suppress("DEPRECATION")
        fun passthrough(enc: Int) = pass?.supportsEncoding(enc) == true
        val audio = mutableListOf(AudioCodecSupport("aac", 8), AudioCodecSupport("mp3", 2))
        if (has("audio/opus")) audio += AudioCodecSupport("opus", 8)
        if (has("audio/flac")) audio += AudioCodecSupport("flac", 8)
        if (has("audio/vorbis")) audio += AudioCodecSupport("vorbis", 8)
        val ac3 = has("audio/ac3") || passthrough(C.ENCODING_AC3)
        val eac3 = has("audio/eac3") || passthrough(C.ENCODING_E_AC3)
        if (ac3) audio += AudioCodecSupport("ac3", 6)
        if (eac3) audio += AudioCodecSupport("eac3", 8)
        if (has("audio/vnd.dts") || passthrough(C.ENCODING_DTS)) audio += AudioCodecSupport("dts", 8)
        if (has("audio/true-hd") || passthrough(C.ENCODING_DOLBY_TRUEHD)) audio += AudioCodecSupport("truehd", 8)
        // PCM lo decodifica sempre ExoPlayer.
        audio += AudioCodecSupport("pcm_s16le", 8)

        // HDR: solo se lo schermo lo mostra davvero (altrimenti il server applica il tone mapping).
        val hdr = mutableListOf<String>()
        if (hevc10 && Build.VERSION.SDK_INT >= 24) {
            val display = runCatching {
                @Suppress("DEPRECATION")
                (context.getSystemService(Context.WINDOW_SERVICE) as WindowManager).defaultDisplay
            }.getOrNull()
            val types = runCatching { display?.hdrCapabilities?.supportedHdrTypes }.getOrNull() ?: intArrayOf()
            if (Display.HdrCapabilities.HDR_TYPE_HDR10 in types) hdr += "hdr10"
            if (Display.HdrCapabilities.HDR_TYPE_HLG in types) hdr += "hlg"
            if (Build.VERSION.SDK_INT >= 29 && Display.HdrCapabilities.HDR_TYPE_HDR10_PLUS in types) hdr += "hdr10plus"
            if (Display.HdrCapabilities.HDR_TYPE_DOLBY_VISION in types && has("video/dolby-vision")) hdr += "dovi"
        }

        val hlsVideo = buildList {
            add("h264")
            if (hevc) add("hevc")
            if (av1) add("av1")
        }
        val hlsAudio = buildList {
            add("aac"); add("mp3")
            if (ac3) add("ac3")
            if (eac3) add("eac3")
        }
        summary = listOf(
            "Video: " + video.joinToString(", ") { v ->
                v.codec + (v.maxBitDepth?.let { if (it > 8) " 10 bit" else "" } ?: "") + (v.maxWidth?.let { " ≤${it}×${v.maxHeight}" } ?: "")
            },
            "Audio: " + audio.filter { it.codec != "pcm_s16le" }.joinToString(", ") { it.codec },
            "HDR: " + (hdr.joinToString(", ").ifEmpty { "—" }),
        )
        return DeviceProfile(
            name = Api.clientName,
            containers = listOf("mp4", "m4v", "mov", "mkv", "matroska", "webm", "ts", "mpegts"),
            videoCodecs = video,
            audioCodecs = audio,
            subtitleText = listOf("vtt"),
            subtitleImage = emptyList(),
            hdr = hdr,
            maxBitrate = null,
            protocols = listOf("file", "hls"),
            hlsVideoCodecs = hlsVideo,
            hlsAudioCodecs = hlsAudio,
        )
    }
}
