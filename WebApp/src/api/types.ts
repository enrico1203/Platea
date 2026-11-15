export interface SystemInfo { name: string; id: string; version: string; product: string; setup_required: boolean; quick_connect?: boolean; login_message?: string | null; custom_css?: string | null }
export interface UserRef { id: string; username: string; is_admin: boolean }
export interface Me extends UserRef {
  device_name: string;
  permissions: { all_libraries: boolean; library_ids: string[]; allow_remux: boolean; allow_transcode: boolean; max_bitrate: number | null; max_streams: number | null; allow_remote: boolean; manage_devices: boolean; expires_at: number | null; expired: boolean; allow_download: boolean; block_unrated: boolean; access_schedule: ScheduleSlot[]; within_schedule: boolean };
  avatar: string | null;
  preferences: UserPrefs;
}
export type ItemKind = "movie" | "series" | "season" | "episode" | "collection";
export interface UserData { position_ms: number; played: boolean; play_count: number; favorite: boolean; last_played_at: number | null }
export interface Item {
  id: string; library_id: string; kind: ItemKind; title: string; original_title: string | null; year: number | null;
  index_number: number | null; index_end: number | null; parent_index_number: number | null; parent_id: string | null; series_id: string | null; series_title: string | null;
  overview: string | null; runtime_ms: number | null; community_rating: number | null; official_rating: string | null; premiere_date: string | null; end_date: string | null; status: string | null;
  available: boolean; match_state: string; added_at: number;
  images: { poster: string | null; poster_color: string | null; backdrop: string | null; still: string | null; logo?: string | null };
  user_data: UserData; child_count: number | null; unplayed_count: number | null;
}
export interface VideoStream { index: number; codec: string; profile: string | null; level: number | null; bit_depth: number | null; width: number | null; height: number | null; frame_rate: number | null; hdr: string | null; bitrate: number | null }
export interface AudioStream { index: number; codec: string; channels: number | null; channel_layout: string | null; bitrate: number | null; language: string | null; title: string | null; default: boolean }
export interface SubtitleStream { index: number; codec: string; language: string | null; title: string | null; default: boolean; forced: boolean; external: boolean; image: boolean }
export interface MediaFile { id: string; name: string; version_label: string | null; container: string | null; size: number; duration_ms: number | null; bitrate: number | null; state: string; probe_state: string; probe_error: string | null; video: VideoStream[]; audio: AudioStream[]; subtitles: SubtitleStream[] }
export interface Person { id: string; name: string; role: string; character: string | null; image: string | null }
export interface ItemDetail extends Item { tagline: string | null; trailers?: { key: string; name: string | null; lang: string | null }[]; manual_collection?: boolean;
  /** Contenuti extra del film o della serie: si riproducono con `/play/<id>?file=<file_id>`. */
  extras?: { file_id: string; title: string; kind: string | null; duration_ms: number | null }[]; genres: string[]; studios: string[]; people: Person[]; external_ids: Record<string, string>; files?: MediaFile[]; locked_fields?: string[]; collections?: { id: string; title: string }[] }
export interface Page<T> { items: T[]; next_cursor?: string | null }
export interface Library {
  id: string; name: string; kind: "movies" | "shows"; item_count: number;
  paths?: string[]; scan_profile?: "local" | "remote"; language?: string; country?: string; scan_interval_minutes?: number | null;
  probe_concurrency?: number; probe_per_minute?: number | null; probe_window_start?: number | null; probe_window_end?: number | null; probe_pause_on_play?: boolean; extract_keyframes?: boolean; trickplay?: boolean; trickplay_done?: number;
  last_scan_at?: number | null; last_scan_status?: string | null; last_scan_message?: string | null; missing_held?: number; file_count?: number; probe_pending?: number;
}
/** Episodio già uscito (secondo TMDB) che non è in libreria. */
export interface MissingEpisode { season: number; episode: number; title: string | null; overview: string | null; air_date: string | null }
export interface Home { resume: Item[]; next_up: Item[]; recent: { library_id: string; library_name: string; kind: string; items: Item[] }[] }
export type Method = "direct_play" | "remux" | "transcode";
export interface PlaybackInfo {
  session_id: string; stream_token: string; method: Method; reasons: string[]; chain: string;
  stream_url: string | null; hls_url: string | null; file_id: string; duration_ms: number; start_ms: number; audio_index: number | null;
  audio_tracks: { index: number; codec: string; channels: number; language: string | null; default: boolean }[];
  subtitles: { index: number; codec: string; language: string | null; title: string | null; external: boolean; forced: boolean; default: boolean; image: boolean; url: string | null }[];
  subtitle: { delivery: string; index?: number };
  subtitle_default: number | null;
  trickplay: Trickplay | null;
  /** Limite di bitrate applicato dal server; null = originale. */
  max_bitrate: number | null;
  container: string;
}
/** Miniature della barra di avanzamento: griglie `cols`×`rows`, una ogni `interval_ms`; `{n}` nell'URL = numero della griglia. */
export interface Trickplay { interval_ms: number; width: number; height: number; cols: number; rows: number; count: number; url: string }
export interface SessionView { id: string; user: string; item_id: string; title: string; subtitle: string | null; poster_url: string | null; device_name: string; client: string; method: Method; reasons: string[]; chain: string; bitrate: number | null; position_ms: number; duration_ms: number; paused: boolean; throttled: boolean; started_at: number;
  server_now: number; position_at: number; paused_since: number | null; last_seen: number; ip: string | null; remote: boolean;
  details: SessionDetails; buffered_until_ms: number | null; backend: string | null; bandwidth_bps: number; bytes_sent: number }
export interface SessionDetails {
  file: { name: string | null; size: number; container: string; bitrate: number | null };
  video: { codec: string; profile: string | null; width: number; height: number; bit_depth: number | null; hdr: string | null; bitrate: number | null } | null;
  audio: { index: number; codec: string; channels: number; language: string | null; bitrate: number | null } | null;
  audio_tracks: number;
  subtitle: { index: number; codec: string; language: string | null; forced: boolean; external: boolean } | null;
  subtitle_tracks: number;
  out: {
    video: { action: "copy" } | { action: "transcode"; codec: string; max_width: number; max_height: number; bitrate: number; tonemap: boolean; burn_subtitle: boolean };
    audio: { action: "copy" } | { action: "none" } | { action: "transcode"; codec: string; channels: number; bitrate: number };
    subtitle: "none" | "external" | "embedded" | "burn_in";
    container: "file" | "hls_fmp4" | "hls_ts";
  };
}
export interface Overview {
  sessions: SessionView[]; bandwidth_bps: number; counts: { direct_play: number; remux: number; transcode: number };
  transcode_slots: { hw_used: number; hw_max: number; sw_used: number; sw_max: number };
  jobs: { queued: number; running: number; failed: number };
  library: { movies: number; series: number; episodes: number; files: number; review: number };
  system: { memory_rss_bytes: number | null; load_1m: number | null; cpus: number; uptime_ms: number };
  alerts: { level: "danger" | "warning" | "info"; code: string; message: string }[];
}
export interface UserAdmin {
  /** Generi nascosti e consentiti (vuoto = tutti). */
  blocked_genres?: string[]; allowed_genres?: string[];
  id: string; username: string; is_admin: boolean; disabled: boolean; all_libraries: boolean; library_ids: string[]; allow_remux: boolean; allow_transcode: boolean;
  max_bitrate: number | null; max_streams: number | null; max_rating: string | null; allow_remote: boolean; manage_devices: boolean; created_at: number; last_login_at: number | null; devices: number; active_streams: number; expires_at: number | null; expired: boolean; allow_download: boolean; block_unrated: boolean; access_schedule: ScheduleSlot[]; avatar: string | null;
}
export interface Device { id: string; device_id: string; device_name: string; client: string; client_version: string; created_at: number; last_used_at: number; last_ip: string | null; current: boolean;
  remote?: boolean | null; playing?: { title: string; subtitle: string | null; method: Method; paused: boolean }; plays?: number; last_played_at?: number | null; last_played_title?: string | null; last_method?: Method | null }
export interface Job { id: number; kind: string; payload: Record<string, unknown>; priority: number; state: string; attempts: number; run_after: number; created_at: number; started_at: number | null; finished_at: number | null; error: string | null; subject?: { title: string; detail: string | null; item_id: string | null } }
export interface Candidate { provider_id: string; title: string; year: number | null; score: number; overview?: string | null; poster: string | null }
export interface ReviewItem { id: string; kind: string; title: string; year: number | null; match_state: string; match_score: number | null; library: string; path: string | null; candidates: Candidate[] }
export interface PlaylistSummary { id: string; name: string; created_at: number; count: number; runtime_ms: number | null; poster: string | null; shared?: boolean; mine?: boolean; owner?: string }
export interface PlaylistDetail { id: string; name: string; created_at: number; runtime_ms: number; items: (Item & { entry_id: string })[]; shared?: boolean; mine?: boolean; owner?: string }

export interface ScheduleSlot { days: number[]; start: number; end: number }
export type SubtitleMode = "default" | "always" | "only_forced" | "none" | "smart";
export interface UserPrefs {
  audio_language: string; subtitle_language: string; subtitle_mode: SubtitleMode;
  autoplay_next: boolean; autoplay_countdown_seconds: number; skip_back_seconds: number; skip_forward_seconds: number;
  subtitle_size: "s" | "m" | "l" | "xl"; subtitle_background: boolean;
  home_sections: string[]; home_hidden_libraries: string[]; hide_played_in_recent: boolean; theme: string; remember_tracks: boolean;
  max_bitrate_local: number; max_bitrate_remote: number;
}
export interface PersonDetail { id: string; name: string; image: string | null; tmdb_id: number | null; items: (Item & { roles: { role: string; character: string | null }[] })[] }

export interface JobProgress {
  scans: { library_id: string; library: string | null; dirs: number; expected_dirs: number | null; percent: number | null; saving: boolean; started_at: number; eta_ms: number | null }[];
  queues: { kind: string; library_id: string | null; library: string | null; queued: number; running: number; done: number; total: number; percent: number; per_minute: number; eta_ms: number | null }[];
  paused_until: number | null;
}
