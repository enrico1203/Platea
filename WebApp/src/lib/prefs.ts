import { api } from "../api/client";
import type { UserPrefs } from "../api/types";

export const DEFAULT_PREFS: UserPrefs = {
  audio_language: "", subtitle_language: "", subtitle_mode: "default",
  autoplay_next: true, autoplay_countdown_seconds: 10, skip_back_seconds: 10, skip_forward_seconds: 30,
  subtitle_size: "m", subtitle_background: false,
  home_sections: ["resume", "next_up", "recent"], home_hidden_libraries: [], hide_played_in_recent: false, theme: "auto", remember_tracks: true, max_bitrate_local: 0, max_bitrate_remote: 0,
};

/** Lingue proposte nelle preferenze (ISO 639-2), con il nome nella lingua dell'interfaccia. */
export const LANGUAGES = ["ita", "eng", "fra", "deu", "spa", "por", "jpn", "kor", "zho", "rus", "nld", "pol", "swe", "dan", "nor", "fin", "ell", "tur", "ara", "heb", "hin"];
const TWO: Record<string, string> = { ita: "it", eng: "en", fra: "fr", deu: "de", spa: "es", por: "pt", jpn: "ja", kor: "ko", zho: "zh", rus: "ru", nld: "nl", pol: "pl", swe: "sv", dan: "da", nor: "no", fin: "fi", ell: "el", tur: "tr", ara: "ar", heb: "he", hin: "hi" };

export function languageName(code: string, uiTag: string): string {
  try {
    const n = new Intl.DisplayNames([uiTag], { type: "language" }).of(TWO[code] ?? code);
    return n ? n.charAt(0).toUpperCase() + n.slice(1) : code;
  } catch { return code; }
}

export async function savePrefs(p: UserPrefs): Promise<UserPrefs> {
  return api<UserPrefs>("/api/v1/me/preferences", { method: "PUT", body: p });
}
