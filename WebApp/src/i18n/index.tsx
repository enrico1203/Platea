/**
 * Traduzioni dell'interfaccia.
 *
 * - Un dizionario per lingua in questa cartella. L'italiano (`it.ts`) è la lingua di riferimento:
 *   le chiavi e la forma degli altri dizionari sono verificate da TypeScript.
 * - Per aggiungere una lingua: creare `xx.ts` con `export default { ... } satisfies Messages`
 *   e registrarla in `locales.ts`. Nessun componente va toccato.
 * - Plurali: una foglia può essere `{ one, other, ... }` e si sceglie con `Intl.PluralRules` dal parametro `count`.
 * - Interpolazione: `{nome}` nel testo, valori passati in `params`.
 * - Numeri, date, durate e bitrate seguono la lingua attiva (`Intl`).
 * - `{app}` è sempre il nome del server (Platea, o quello scelto dall'amministratore): mai scrivere "Platea" nei testi.
 */
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import it from "./it";
import { LOCALES, type LocaleCode } from "./locales";

export type Plural = { zero?: string; one: string; two?: string; few?: string; many?: string; other: string };
type Leaf = string | Plural;
export type Dict = { [k: string]: Leaf | Dict };

/** Forma dei dizionari: stessa struttura dell'italiano, valori stringa o plurali. */
type Widen<T> = T extends string ? string : T extends Plural ? Plural : { [K in keyof T]: Widen<T[K]> };
export type Messages = Widen<typeof it>;

/** Tutti i percorsi delle foglie: "player.play", "admin.users.title"... */
type Paths<T, P extends string = ""> = {
  [K in keyof T & string]: T[K] extends string | Plural ? `${P}${K}` : Paths<T[K], `${P}${K}.`>;
}[keyof T & string];
export type MessageKey = Paths<typeof it>;

export type Params = Record<string, string | number | null | undefined>;

const STORAGE_KEY = "platea.lang";

function lookup(dict: Dict, key: string): Leaf | undefined {
  let cur: Leaf | Dict | undefined = dict;
  for (const part of key.split(".")) {
    if (cur == null || typeof cur === "string") return undefined;
    cur = (cur as Dict)[part];
  }
  return cur as Leaf | undefined;
}

function interpolate(s: string, params?: Params): string {
  if (!params) return s;
  return s.replace(/\{(\w+)\}/g, (m, name) => (params[name] == null ? m : String(params[name])));
}

export function detectLocale(): LocaleCode {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && saved in LOCALES) return saved as LocaleCode;
  } catch { /* archiviazione non disponibile */ }
  for (const l of navigator.languages ?? [navigator.language]) {
    const short = l.toLowerCase().split("-")[0];
    if (short in LOCALES) return short as LocaleCode;
  }
  return "it";
}

export interface I18n {
  locale: LocaleCode;
  /** Tag BCP 47 per Intl (es. it-IT). */
  tag: string;
  setLocale: (l: LocaleCode) => void;
  /** Nome del server per `{app}` nei testi; lo imposta la sessione. */
  setAppName: (name: string) => void;
  t: (key: MessageKey, params?: Params) => string;
  num: (n: number, digits?: number) => string;
  bytes: (n: number | null | undefined) => string;
  bitrate: (bps: number | null | undefined) => string;
  duration: (ms: number | null | undefined) => string;
  clock: (ms: number) => string;
  date: (d: string | number | null | undefined) => string;
  dateTime: (d: number | null | undefined) => string;
  relative: (ms: number | null | undefined) => string;
}

const Ctx = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<LocaleCode>(detectLocale);
  const [dict, setDict] = useState<Dict | null>(locale === "it" ? (it as unknown as Dict) : null);

  useEffect(() => {
    let alive = true;
    LOCALES[locale].load().then((d) => alive && setDict(d as unknown as Dict));
    document.documentElement.lang = locale;
    return () => { alive = false; };
  }, [locale]);

  const [appName, setAppNameState] = useState("Platea");
  const setAppName = useCallback((n: string) => setAppNameState(n.trim() || "Platea"), []);

  const setLocale = useCallback((l: LocaleCode) => {
    try { localStorage.setItem(STORAGE_KEY, l); } catch { /* ignora */ }
    setLocaleState(l);
  }, []);

  const value = useMemo<I18n | null>(() => {
    if (!dict) return null;
    const tag = LOCALES[locale].tag;
    const plural = new Intl.PluralRules(tag);
    const fallback = it as unknown as Dict;
    const t = (key: MessageKey, p?: Params) => {
      const params: Params = { app: appName, ...p };
      const leaf = lookup(dict, key) ?? lookup(fallback, key);
      if (leaf == null) return key;
      if (typeof leaf === "string") return interpolate(leaf, params);
      const count = Number(params?.count ?? 0);
      const form = plural.select(count) as keyof Plural;
      return interpolate(leaf[form] ?? leaf.other, { ...params, count: new Intl.NumberFormat(tag).format(count) });
    };
    const nf = (digits = 0) => new Intl.NumberFormat(tag, { maximumFractionDigits: digits, minimumFractionDigits: digits });
    const num = (n: number, digits = 0) => nf(digits).format(n);
    const bytes = (n: number | null | undefined) => {
      if (n == null) return "—";
      const units = ["B", "KB", "MB", "GB", "TB", "PB"];
      let v = n;
      let i = 0;
      while (v >= 1000 && i < units.length - 1) { v /= 1000; i++; }
      return `${nf(i === 0 ? 0 : 1).format(v)} ${units[i]}`;
    };
    const bitrate = (bps: number | null | undefined) => {
      if (!bps) return "—";
      if (bps >= 1_000_000) return `${nf(1).format(bps / 1_000_000)} Mb/s`;
      return `${nf(0).format(bps / 1000)} kb/s`;
    };
    const duration = (ms: number | null | undefined) => {
      if (!ms || ms <= 0) return "—";
      const min = Math.round(ms / 60000);
      const h = Math.floor(min / 60);
      const m = min % 60;
      return h > 0 ? (m > 0 ? `${h} h ${m} min` : `${h} h`) : `${Math.max(m, 1)} min`;
    };
    const clock = (ms: number) => {
      const s = Math.max(0, Math.floor(ms / 1000));
      const h = Math.floor(s / 3600);
      const m = Math.floor((s % 3600) / 60);
      const sec = s % 60;
      const pad = (x: number) => String(x).padStart(2, "0");
      return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
    };
    const date = (d: string | number | null | undefined) => {
      if (d == null || d === "") return "—";
      const dt = typeof d === "number" ? new Date(d) : new Date(d.length === 10 ? `${d}T00:00:00` : d);
      return Number.isNaN(dt.getTime()) ? String(d) : new Intl.DateTimeFormat(tag, { day: "numeric", month: "long", year: "numeric" }).format(dt);
    };
    const dateTime = (d: number | null | undefined) =>
      d == null ? "—" : new Intl.DateTimeFormat(tag, { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(d));
    const rtf = new Intl.RelativeTimeFormat(tag, { numeric: "auto" });
    const relative = (ms: number | null | undefined) => {
      if (ms == null) return "—";
      const diff = (ms - Date.now()) / 1000;
      const abs = Math.abs(diff);
      if (abs < 60) return rtf.format(Math.round(diff), "second");
      if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
      if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
      return rtf.format(Math.round(diff / 86400), "day");
    };
    return { locale, tag, setLocale, setAppName, t, num, bytes, bitrate, duration, clock, date, dateTime, relative };
  }, [dict, locale, setLocale, setAppName, appName]);

  if (!value) return null;
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useI18n(): I18n {
  const v = useContext(Ctx);
  if (!v) throw new Error("useI18n fuori da I18nProvider");
  return v;
}

export function useT() {
  return useI18n().t;
}
