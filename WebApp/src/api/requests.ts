/**
 * Corpi delle richieste e risposte dell'API che la web UI costruisce al volo, in un posto solo.
 * Insieme a `types.ts` sono la fonte degli schemi del documento OpenAPI (`scripts/openapi-schemas.sh`),
 * da cui si generano i client per le app (Kotlin, Swift).
 */
import type { DeviceProfile } from "../lib/profile";
import type { Candidate, Device, Item, Library, MissingEpisode, Page, PlaylistSummary, ReviewItem, UserAdmin, UserRef } from "./types";

export type { DeviceProfile };

/** Pagine concrete (il documento OpenAPI non ha tipi generici). */
export type ItemPage = Page<Item>;
export type LibraryPage = Page<Library>;
export type UserAdminPage = Page<UserAdmin>;
export interface DeviceList { items: Device[] }
export interface MissingEpisodeList { items: MissingEpisode[] }
export interface CandidateList { items: Candidate[] }
export interface ReviewList { items: ReviewItem[] }
export interface PlaylistList { items: PlaylistSummary[] }

export interface Ok { ok: boolean }

export interface LoginRequest { username: string; password: string; device_id?: string; device_name?: string; client?: string; client_version?: string }
export interface LoginResponse { token: string; user: UserRef }
export interface PasswordChangeRequest { current: string; new: string }

export interface PlaybackInfoRequest {
  item_id: string;
  /** Versione del file; omessa = la migliore. */
  file_id?: string;
  /** Ciò che il dispositivo sa riprodurre: il server decide direct play, remux o transcodifica. */
  profile: DeviceProfile;
  audio_index?: number;
  subtitle_index?: number;
  start_ms?: number;
  max_bitrate?: number;
  force_transcode?: boolean;
  /** Lingua dell'interfaccia ("it"): per i sottotitoli quando l'utente non ne ha scelta una. */
  ui_language?: string;
  /** Qualità scelta a mano nel player: la qualità predefinita delle preferenze non si applica. */
  explicit_quality?: boolean;
}
export interface ProgressRequest { position_ms: number; paused?: boolean }
export interface TracksRequest { audio_index?: number | null; subtitle_index?: number | null; subtitle_off?: boolean }

export interface PlaylistCreateRequest { name: string; item_ids?: string[] }
export interface PlaylistCreateResponse { id: string; name: string; added: number }
export interface PlaylistUpdateRequest { name?: string; shared?: boolean }
export interface PlaylistAddRequest { item_ids: string[] }
export interface PlaylistMoveRequest { entry_id: string; index: number }

export interface QuickConnectInitiateRequest { device_id?: string; device_name?: string; client?: string; client_version?: string }
export interface QuickConnectInitiateResponse { code: string; secret: string; expires_at: number }
export interface QuickConnectStatus { state: "pending" | "authorized"; expires_at: number }
export interface QuickConnectRedeemRequest { secret: string }
export interface QuickConnectAuthorizeRequest { code: string }
