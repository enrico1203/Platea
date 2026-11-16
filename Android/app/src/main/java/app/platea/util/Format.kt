package app.platea.util

import java.text.DateFormat
import java.text.NumberFormat
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** Formati che dipendono dalla lingua del sistema (come `Intl` nella web UI). */
object Fmt {
    /** 1 h 52 min · 45 min */
    fun duration(ms: Long): String {
        val min = ((ms + 30_000) / 60_000).toInt()
        val h = min / 60
        val m = min % 60
        return when {
            h > 0 && m > 0 -> "$h h $m min"
            h > 0 -> "$h h"
            m == 0 -> "< 1 min"
            else -> "$m min"
        }
    }

    /** 1:52:07 · 4:05 */
    fun clock(ms: Long): String {
        val s = (ms.coerceAtLeast(0) / 1000).toInt()
        val h = s / 3600
        val m = (s % 3600) / 60
        val sec = s % 60
        return if (h > 0) "%d:%02d:%02d".format(h, m, sec) else "%d:%02d".format(m, sec)
    }

    fun bitrate(bps: Long): String = when {
        bps >= 1_000_000 -> NumberFormat.getNumberInstance().apply { maximumFractionDigits = 1 }.format(bps / 1_000_000.0) + " Mb/s"
        else -> "${bps / 1000} kb/s"
    }

    fun size(bytes: Long): String {
        val units = listOf("B", "KB", "MB", "GB", "TB")
        var v = bytes.toDouble()
        var i = 0
        while (v >= 1024 && i < units.size - 1) { v /= 1024; i++ }
        return NumberFormat.getNumberInstance().apply { maximumFractionDigits = if (i >= 3) 2 else 0 }.format(v) + " " + units[i]
    }

    fun rating(r: Double): String = "★ " + NumberFormat.getNumberInstance().apply { maximumFractionDigits = 1; minimumFractionDigits = 1 }.format(r)

    /** Data da millisecondi. */
    fun date(ms: Long): String = DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(ms))

    /** Data ISO "2024-05-17". */
    fun isoDate(s: String): String = runCatching {
        DateFormat.getDateInstance(DateFormat.MEDIUM).format(SimpleDateFormat("yyyy-MM-dd", Locale.US).parse(s.take(10))!!)
    }.getOrDefault(s)

    fun dateTime(ms: Long): String = DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(ms))

    /** Nome di una lingua da codice ISO 639-2 o 639-1, nella lingua del sistema. */
    fun language(code: String?): String {
        if (code.isNullOrBlank()) return "?"
        val two = ISO3[code.lowercase()] ?: code
        val n = Locale(two).getDisplayLanguage(Locale.getDefault())
        return if (n.isBlank() || n.equals(two, ignoreCase = true)) code.uppercase() else n.replaceFirstChar { it.titlecase(Locale.getDefault()) }
    }

    fun channels(c: Int): String = if (c > 2) "${c - 1}.1" else "$c.0"

    /** Lingue proposte nelle preferenze (ISO 639-2), come la web UI. */
    val LANGUAGES = listOf("ita", "eng", "fra", "deu", "spa", "por", "jpn", "kor", "zho", "rus", "nld", "pol", "swe", "dan", "nor", "fin", "ell", "tur", "ara", "heb", "hin")
    private val ISO3 = mapOf(
        "ita" to "it", "eng" to "en", "fra" to "fr", "fre" to "fr", "deu" to "de", "ger" to "de", "spa" to "es", "por" to "pt",
        "jpn" to "ja", "kor" to "ko", "zho" to "zh", "chi" to "zh", "rus" to "ru", "nld" to "nl", "dut" to "nl", "pol" to "pl",
        "swe" to "sv", "dan" to "da", "nor" to "no", "fin" to "fi", "ell" to "el", "gre" to "el", "tur" to "tr", "ara" to "ar",
        "heb" to "he", "hin" to "hi", "ces" to "cs", "cze" to "cs", "hun" to "hu", "ron" to "ro", "rum" to "ro", "ukr" to "uk",
        "tha" to "th", "vie" to "vi", "ind" to "id", "msa" to "ms", "may" to "ms", "cat" to "ca", "hrv" to "hr", "srp" to "sr",
        "slk" to "sk", "slo" to "sk", "slv" to "sl", "bul" to "bg", "est" to "et", "lav" to "lv", "lit" to "lt", "isl" to "is",
        "ice" to "is", "fas" to "fa", "per" to "fa",
    )

    /** "it" per l'italiano: le altre lingue usano "en" come la web UI. */
    fun uiLanguage(): String = if (Locale.getDefault().language == "it") "it" else "en"
}
