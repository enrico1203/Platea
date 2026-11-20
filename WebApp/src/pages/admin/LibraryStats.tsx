import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, RefreshCw } from "lucide-react";
import { api, ApiError } from "../../api/client";
import type { LibraryStats, LibraryStatsLibrary, LibraryStatsRow } from "../../api/types";
import { Alert, Empty } from "../../components/ui";
import { LineChart } from "../../components/LineChart";
import { ChartCard, Columns, HBars, pctText, Segmented, StackRows, type Category, type Slot } from "../../components/StatCharts";
import { languageName } from "../../lib/prefs";
import { useI18n, type MessageKey } from "../../i18n";

type Metric = "size" | "files" | "duration";
type Agg = { files: number; size: number; duration_ms: number };
type Dim = keyof LibraryStats["breakdowns"];

const ZERO: Agg = { files: 0, size: 0, duration_ms: 0 };
const val = (a: Agg | undefined, m: Metric) => (!a ? 0 : m === "size" ? a.size : m === "files" ? a.files : a.duration_ms);
const OTHER = "__other";

/** Somma le righe di una dimensione per chiave, per una libreria o per tutte (`lib` vuoto). */
function sumKeys(rows: LibraryStatsRow[] | undefined, lib: string): Map<string, Agg> {
  const out = new Map<string, Agg>();
  for (const r of rows ?? []) {
    if (lib && r.lib !== lib) continue;
    const a = out.get(r.key) ?? { ...ZERO };
    a.files += r.files; a.size += r.size; a.duration_ms += r.duration_ms;
    out.set(r.key, a);
  }
  return out;
}

const CODECS: Record<string, string> = {
  hevc: "HEVC", h264: "H.264", av1: "AV1", mpeg4: "MPEG-4", mpeg2video: "MPEG-2", vc1: "VC-1", vp9: "VP9", vp8: "VP8", wmv3: "WMV3", msmpeg4v3: "MS-MPEG4", mpeg1video: "MPEG-1",
  aac: "AAC", ac3: "Dolby Digital", eac3: "Dolby Digital+", dts: "DTS", truehd: "Dolby TrueHD", flac: "FLAC", mp3: "MP3", mp2: "MP2", opus: "Opus", vorbis: "Vorbis", alac: "ALAC", wmav2: "WMA",
};
const codecLabel = (c: string) => CODECS[c] ?? (c.startsWith("pcm_") ? "PCM" : c.toUpperCase());
const HDR: Record<string, string> = { sdr: "SDR", hdr10: "HDR10", hdr10plus: "HDR10+", dovi: "Dolby Vision", hlg: "HLG" };
/** Codici ISO 639-2 bibliografici e varianti: nome della lingua tramite il codice a due lettere. */
const LANG_ALIAS: Record<string, string> = {
  fre: "fr", fra: "fr", ger: "de", deu: "de", chi: "zh", zho: "zh", dut: "nl", nld: "nl", gre: "el", ell: "el", cze: "cs", ces: "cs", hun: "hu", rum: "ro", ron: "ro",
  tha: "th", vie: "vi", ukr: "uk", ind: "id", may: "ms", msa: "ms", cat: "ca", hrv: "hr", srp: "sr", bul: "bg", slv: "sl", slo: "sk", slk: "sk", est: "et", lav: "lv",
  lit: "lt", ice: "is", isl: "is", per: "fa", fas: "fa", tam: "ta", tel: "te", fil: "fil", tgl: "tl", heb: "he", ara: "ar", hin: "hi", tur: "tr", pol: "pl", swe: "sv",
  dan: "da", nor: "no", nob: "nb", fin: "fi", rus: "ru", kor: "ko", jpn: "ja", por: "pt", spa: "es", eng: "en", ita: "it", lat: "la", gle: "ga", wel: "cy", baq: "eu", glg: "gl",
};

export default function LibraryStatsPage() {
  const { t, tag, num, bytes, bitrate, duration, relative } = useI18n();
  const qc = useQueryClient();
  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: ["libraries", "stats"],
    queryFn: () => api<LibraryStats>("/api/v1/libraries/stats"),
    staleTime: 5 * 60_000,
  });
  const [refreshing, setRefreshing] = useState(false);
  const [metric, setMetric] = useState<Metric>("size");
  const [lib, setLib] = useState("");

  const refresh = async () => {
    setRefreshing(true);
    try { qc.setQueryData(["libraries", "stats"], await api<LibraryStats>("/api/v1/libraries/stats?refresh=true")); }
    finally { setRefreshing(false); }
  };

  /** Durata lunga: ore fino a tre giorni, poi giorni. */
  const longDuration = (ms: number) => {
    const h = ms / 3_600_000;
    if (h < 1) return duration(ms);
    if (h < 72) return `${num(h)} h`;
    return t("admin.stats.days", { count: Math.round(h / 24) });
  };
  const fmt = (m: Metric) => (m === "size" ? bytes : m === "files" ? (v: number) => num(v) : longDuration);
  const axisFmt = (m: Metric) => (m === "size" ? bytes : m === "files" ? (v: number) => num(v) : (v: number) => `${num(v / 3_600_000)} h`);

  const libs = data?.libraries ?? [];
  const libName = useMemo(() => new Map(libs.map((l) => [l.id, l.name])), [libs]);
  const b = data?.breakdowns ?? {};

  /** Categorie di una dimensione, con i colori in ordine fisso: elenco dato, oppure le più grandi in assoluto. */
  const categories = (dim: Dim, label: (k: string) => string, opts: { order?: string[]; max?: number; by?: Metric; other?: string[] } = {}) => {
    const totals = sumKeys(b[dim], "");
    const by = opts.by ?? "size";
    let keys: string[];
    if (opts.order) keys = opts.order.filter((k) => totals.has(k));
    else keys = [...totals.entries()].filter(([k]) => !(opts.other ?? []).includes(k)).sort((x, y) => val(y[1], by) - val(x[1], by)).slice(0, opts.max ?? 5).map(([k]) => k);
    const cats: Category[] = keys.map((k, i) => ({ key: k, label: label(k), slot: (i + 1) as Slot }));
    const hasOther = [...totals.keys()].some((k) => !keys.includes(k));
    if (hasOther) cats.push({ key: OTHER, label: t("admin.stats.other"), slot: "other" });
    const map = (k: string) => (keys.includes(k) ? k : OTHER);
    return { cats, map };
  };

  /** Righe di composizione: il totale e poi ogni libreria che ha dati in quella dimensione. */
  const stackRows = (dim: Dim, map: (k: string) => string, m: Metric) => {
    const row = (l: string) => {
      const values: Record<string, number> = {};
      for (const [k, a] of sumKeys(b[dim], l)) values[map(k)] = (values[map(k)] ?? 0) + val(a, m);
      return values;
    };
    const withData = [...libs].sort((x, y) => y.size - x.size).filter((l) => (b[dim] ?? []).some((r) => r.lib === l.id));
    return [
      ...(withData.length > 1 ? [{ key: "_all", label: t("admin.stats.allLibraries"), values: row(""), strong: true }] : []),
      ...withData.map((l) => ({ key: l.id, label: l.name, values: row(l.id) })),
    ];
  };

  const sum = (f: (l: LibraryStatsLibrary) => number, only?: (l: LibraryStatsLibrary) => boolean) => libs.filter(only ?? (() => true)).reduce((a, l) => a + f(l), 0);

  if (isLoading) {
    return (
      <div className="stack-lg">
        <Head />
        <div className="pl-panel panel-pad row" role="status"><RefreshCw className="spin" /> {t("admin.stats.computing")}</div>
      </div>
    );
  }
  if (isError || !data) {
    return (
      <div className="stack-lg">
        <Head />
        <Alert tone="danger">{(error as ApiError)?.message || t("errors.network")} <button className="pl-btn pl-btn--ghost" onClick={() => refetch()}>{t("common.retry")}</button></Alert>
      </div>
    );
  }
  if (!libs.length) return <div className="stack-lg"><Head /><Empty>{t("home.emptyAdmin")}</Empty></div>;

  function Head() {
    return (
      <div className="page-head" style={{ marginBottom: 0, flexWrap: "wrap", gap: "var(--space-3)" }}>
        <div className="stack" style={{ gap: 4 }}>
          <Link to="/admin/libraries" className="t-small row" style={{ gap: 4 }}><ArrowLeft size={14} />{t("admin.libraries.title")}</Link>
          <h1 className="t-h1">{t("admin.stats.title")}</h1>
          {data && <div className="t-small subtle">{t("admin.stats.generated", { when: relative(data.generated_at), secs: num(data.took_ms / 1000, 1) })}</div>}
        </div>
        {data && <button className="pl-btn pl-btn--secondary" onClick={refresh} disabled={refreshing}><RefreshCw className={refreshing ? "spin" : undefined} />{t("admin.stats.refresh")}</button>}
      </div>
    );
  }

  // ---- Riepilogo
  const total = sum((l) => l.size);
  const files = sum((l) => l.files);
  const dur = sum((l) => l.duration_ms);
  const movies = sum((l) => l.movies);
  const movieSize = sum((l) => l.video_size, (l) => l.kind === "movies");
  const series = sum((l) => l.series);
  const seasons = sum((l) => l.seasons);
  const episodes = sum((l) => l.episodes);
  const audiobooks = sum((l) => l.audiobooks);
  const videoFiles = sum((l) => l.video_files);
  const videoSize = sum((l) => l.video_size);
  const videoDur = sum((l) => l.video_duration_ms);
  const avgBitrate = videoFiles ? sum((l) => l.avg_bitrate * l.video_files) / videoFiles : 0;
  const plays = sum((l) => l.plays);
  const watched = sum((l) => l.watched_ms);
  const res = sumKeys(b.resolution, "");
  const share = (m: Map<string, Agg>, keys: string[], metricK: Metric, base: number) => (base > 0 ? (keys.reduce((a, k) => a + val(m.get(k), metricK), 0) / base) * 100 : 0);
  const uhdShare = share(res, ["2160p"], "size", videoSize);
  const hdr = sumKeys(b.hdr, "");
  const hdrShare = share(hdr, ["hdr10", "hdr10plus", "dovi", "hlg"], "files", videoFiles);
  const vcodec = sumKeys(b.video_codec, "");
  const modernShare = share(vcodec, ["hevc", "av1"], "files", videoFiles);
  const gbPerHour = (size: number, ms: number) => (ms > 0 ? size / (ms / 3_600_000) : 0);
  const years = dur / 3_600_000 / 24 / 365;

  // ---- Confronto
  const cmpRows = [...libs]
    .map((l) => ({ key: l.id, label: <>{l.name}</>, value: val(l, metric), note: kindShort(l.kind) }))
    .sort((x, y) => y.value - x.value);
  function kindShort(k: LibraryStatsLibrary["kind"]) { return k === "movies" ? t("library.movies") : k === "audiobooks" ? t("library.audiobooks") : t("library.shows"); }
  const libRes = (id: string) => sumKeys(b.resolution, id);
  const libCodec = (id: string) => sumKeys(b.video_codec, id);

  // ---- Composizione
  const resLabel = (k: string) => t(`admin.stats.res.${k}` as MessageKey);
  const resCats = categories("resolution", resLabel, { order: ["2160p", "1080p", "720p", "sd", "audio"] });
  const codecCats = categories("video_codec", codecLabel, { max: 5 });
  const hdrCats = categories("hdr", (k) => HDR[k] ?? k.toUpperCase(), { order: ["sdr", "hdr10", "hdr10plus", "dovi", "hlg"] });
  const acodecCats = categories("audio_codec", codecLabel, { max: 6, by: "files" });
  const chCats = categories("channels", (k) => (k === "mono" || k === "stereo" ? t(`admin.stats.ch.${k}` as MessageKey) : k), { order: ["stereo", "5.1", "7.1", "mono", "6.1"] });
  const contCats = categories("container", (k) => k.toUpperCase(), { max: 5, other: ["unknown"] });
  const matchCats = categories("match", (k) => t(`admin.stats.matchState.${k}` as MessageKey), { order: ["matched", "manual", "review", "unmatched", "pending"], by: "files" });

  // ---- Dettaglio (una libreria o tutte)
  const sel = lib ? libs.filter((l) => l.id === lib) : libs;
  const growth = (() => {
    const m = sumKeys(b.growth, lib);
    const keys = [...m.keys()].filter((k) => k >= "1990").sort();
    if (!keys.length) return { times: [] as number[], cum: [] as number[], bars: [] as { key: string; label: string; tip: string; value: number }[] };
    const all = keys.reduce((a, k) => a + val(m.get(k), metric), 0);
    // Parte dal primo mese che porta qualcosa di significativo (pochi file vecchi allungherebbero l'asse per niente).
    let start = 0;
    for (let acc = 0; start < keys.length; start++) { acc += val(m.get(keys[start]), metric); if (acc >= all * 0.002) break; }
    const before = keys.slice(0, start).reduce((a, k) => a + val(m.get(k), metric), 0);
    const [y0, m0] = keys[start].split("-").map(Number);
    const [y1, m1] = keys[keys.length - 1].split("-").map(Number);
    const times: number[] = [], cum: number[] = [], bars: { key: string; label: string; tip: string; value: number }[] = [];
    const short = new Intl.DateTimeFormat(tag, { month: "short", year: "2-digit", timeZone: "UTC" });
    const long = new Intl.DateTimeFormat(tag, { month: "long", year: "numeric", timeZone: "UTC" });
    let acc = before;
    for (let y = y0, mo = m0; y < y1 || (y === y1 && mo <= m1); mo === 12 ? (y++, mo = 1) : mo++) {
      const k = `${y}-${String(mo).padStart(2, "0")}`;
      const v = val(m.get(k), metric);
      acc += v;
      const ts = Date.UTC(y, mo - 1, 1);
      times.push(ts); cum.push(acc);
      bars.push({ key: k, label: short.format(ts), tip: long.format(ts), value: v });
    }
    return { times, cum, bars };
  })();
  const monthLabel = (ts: number) => new Intl.DateTimeFormat(tag, { month: "short", year: "2-digit", timeZone: "UTC" }).format(ts);

  const decades = [...sumKeys(b.decade, lib).entries()].filter(([k]) => k !== "unknown").sort((x, y) => Number(x[0]) - Number(y[0]))
    .map(([k, a]) => ({ key: k, label: k, tip: `${k}–${Number(k) + 9}`, value: val(a, metric) }));
  const BITRATES = ["0-2", "2-5", "5-10", "10-20", "20-40", "40+"];
  const br = sumKeys(b.bitrate, lib);
  const bitrateBars = BITRATES.map((k) => ({ key: k, label: k === "40+" ? "40+" : k.replace("-", "–"), tip: `${k === "40+" ? "> 40" : k.replace("-", "–")} Mbps`, value: br.get(k)?.files ?? 0 }));
  const ratings = sumKeys(b.rating, lib);
  const ratingBars = Array.from({ length: 10 }, (_, i) => i + 1).map((r) => ({
    key: String(r), label: String(r), tip: r === 10 ? "10" : `${r}–${num(r + 0.9, 1)}`,
    value: (ratings.get(String(r))?.files ?? 0) + (r === 1 ? ratings.get("0")?.files ?? 0 : 0),
  }));
  const selFiles = sel.reduce((a, l) => a + l.files, 0);
  const langRows = (dim: Dim) => [...sumKeys(b[dim], lib).entries()].sort((x, y) => y[1].files - x[1].files).slice(0, 12)
    .map(([k, a]) => ({ key: k, label: k === "und" ? t("admin.stats.langUnknown") : languageName(LANG_ALIAS[k] ?? k, tag), value: a.files }));
  const genreRows = [...sumKeys(b.genre, lib).entries()].sort((x, y) => y[1].files - x[1].files).slice(0, 15).map(([k, a]) => ({ key: k, label: k, value: a.files }));
  const titles = sel.reduce((a, l) => a + l.movies + l.series + l.audiobooks, 0);
  const heavy = (data.heaviest ?? []).filter((h) => !lib || h.library === lib).slice(0, 12);
  const s = (f: (l: LibraryStatsLibrary) => number) => sel.reduce((a, l) => a + f(l), 0);
  const unknownRes = sumKeys(b.resolution, lib).get("unknown")?.files ?? 0;

  const metricOpts = [
    { value: "size" as const, label: t("admin.stats.metric.size") },
    { value: "files" as const, label: t("admin.stats.metric.files") },
    { value: "duration" as const, label: t("admin.stats.metric.duration") },
  ];
  const metricNote = t(`admin.stats.by.${metric}` as MessageKey);

  return (
    <div className="stack-lg">
      <Head />

      {/* Riepilogo */}
      <div className="sc-kpis">
        <div className="stat sc-kpi sc-kpi--hero">
          <span className="pl-eyebrow">{t("admin.stats.kpi.size")}</span>
          <span className="t-numeral">{bytes(total)}</span>
          <span className="t-small subtle">{t("admin.stats.kpi.sizeSub", { files: num(files), libs: libs.length, avg: bytes(files ? total / files : 0) })}</span>
        </div>
        <div className="stat sc-kpi">
          <span className="pl-eyebrow">{t("admin.stats.kpi.duration")}</span>
          <span className="t-numeral">{dur >= 72 * 3_600_000 ? <>{num(Math.round(dur / 86_400_000))}<small>{t("admin.stats.daysUnit")}</small></> : longDuration(dur)}</span>
          <span className="t-small subtle">{years >= 1 ? t("admin.stats.kpi.years", { n: num(years, 1) }) : t("admin.stats.kpi.hours", { n: num(dur / 3_600_000) })}</span>
        </div>
        {movies > 0 && (
          <div className="stat sc-kpi">
            <span className="pl-eyebrow">{t("library.movies")}</span>
            <span className="t-numeral">{num(movies)}</span>
            <span className="t-small subtle">{t("admin.stats.kpi.perMovie", { size: bytes(movieSize / movies) })}</span>
          </div>
        )}
        {series > 0 && (
          <div className="stat sc-kpi">
            <span className="pl-eyebrow">{t("library.shows")}</span>
            <span className="t-numeral">{num(series)}</span>
            <span className="t-small subtle">{t("admin.stats.kpi.seriesSub", { seasons: num(seasons), episodes: num(episodes) })}</span>
          </div>
        )}
        {audiobooks > 0 && (
          <div className="stat sc-kpi">
            <span className="pl-eyebrow">{t("library.audiobooks")}</span>
            <span className="t-numeral">{num(audiobooks)}</span>
            <span className="t-small subtle">{bytes(sum((l) => l.size, (l) => l.kind === "audiobooks"))}</span>
          </div>
        )}
        <div className="stat sc-kpi">
          <span className="pl-eyebrow">{t("admin.stats.kpi.uhd")}</span>
          <span className="t-numeral">{pctText(uhdShare)}</span>
          <span className="t-small subtle">{t("admin.stats.kpi.uhdSub", { hdr: pctText(hdrShare), dv: num(hdr.get("dovi")?.files ?? 0) })}</span>
        </div>
        <div className="stat sc-kpi">
          <span className="pl-eyebrow">{t("admin.stats.kpi.modern")}</span>
          <span className="t-numeral">{pctText(modernShare)}</span>
          <span className="t-small subtle">{t("admin.stats.kpi.modernSub", { h264: pctText(share(vcodec, ["h264"], "files", videoFiles)) })}</span>
        </div>
        <div className="stat sc-kpi">
          <span className="pl-eyebrow">{t("admin.stats.kpi.bitrate")}</span>
          <span className="t-numeral">{bitrate(avgBitrate)}</span>
          <span className="t-small subtle">{t("admin.stats.kpi.perHour", { size: bytes(gbPerHour(videoSize, videoDur)) })}</span>
        </div>
        <div className="stat sc-kpi">
          <span className="pl-eyebrow">{t("admin.stats.kpi.plays")}</span>
          <span className="t-numeral">{num(plays)}</span>
          <span className="t-small subtle">{t("admin.stats.kpi.watched", { time: longDuration(watched) })}</span>
        </div>
      </div>

      {/* Confronto tra librerie */}
      <section className="sc-section">
        <div className="sc-section__head">
          <h2 className="sc-section__title">{t("admin.stats.compare")}</h2>
          <Segmented label={t("admin.stats.metric.label")} value={metric} onChange={setMetric} options={metricOpts} />
        </div>
        <ChartCard title={t(`admin.stats.metric.${metric}` as MessageKey)} note={t("admin.stats.compareNote")} wide>
          <HBars rows={cmpRows} format={fmt(metric)} />
        </ChartCard>
        <div className="pl-panel">
          <div className="table-wrap">
            <table className="table sc-table">
              <thead>
                <tr>
                  <th>{t("admin.stats.col.library")}</th>
                  <th className="num">{t("admin.stats.metric.size")}</th>
                  <th className="num">{t("admin.stats.metric.files")}</th>
                  <th className="num">{t("admin.stats.col.titles")}</th>
                  <th className="num">{t("admin.stats.metric.duration")}</th>
                  <th className="num">{t("admin.stats.col.avgFile")}</th>
                  <th className="num">{t("admin.stats.col.perHour")}</th>
                  <th className="num">{t("admin.stats.col.bitrate")}</th>
                  <th className="num">4K</th>
                  <th className="num">HEVC/AV1</th>
                </tr>
              </thead>
              <tbody>
                {[...libs].sort((x, y) => y.size - x.size).map((l) => {
                  const r = libRes(l.id), c = libCodec(l.id);
                  const video = l.video_files > 0 && l.kind !== "audiobooks";
                  return (
                    <tr key={l.id}>
                      <td><div style={{ whiteSpace: "nowrap" }}>{l.name}</div><div className="t-small subtle">{kindShort(l.kind)}</div></td>
                      <td className="num t-mono-sm">{bytes(l.size)}<div className="t-small subtle">{pctText(total ? (l.size / total) * 100 : 0)}</div></td>
                      <td className="num t-mono-sm">{num(l.files)}</td>
                      <td className="num t-mono-sm">{num(l.movies + l.series + l.audiobooks)}{l.episodes > 0 && <div className="t-small subtle">{t("admin.stats.episodesShort", { count: l.episodes })}</div>}</td>
                      <td className="num t-mono-sm">{longDuration(l.duration_ms)}</td>
                      <td className="num t-mono-sm">{bytes(l.files ? l.size / l.files : 0)}</td>
                      <td className="num t-mono-sm">{video ? bytes(gbPerHour(l.video_size, l.video_duration_ms)) : "—"}</td>
                      <td className="num t-mono-sm">{video ? bitrate(l.avg_bitrate) : "—"}</td>
                      <td className="num t-mono-sm">{video ? pctText(share(r, ["2160p"], "files", l.video_files)) : "—"}</td>
                      <td className="num t-mono-sm">{video ? pctText(share(c, ["hevc", "av1"], "files", l.video_files)) : "—"}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      </section>

      {/* Composizione per libreria */}
      <section className="sc-section">
        <div className="sc-section__head">
          <h2 className="sc-section__title">{t("admin.stats.quality")}</h2>
          <span className="t-small subtle">{metricNote}</span>
        </div>
        <div className="sc-grid">
          <ChartCard title={t("admin.stats.resolution")}><StackRows rows={stackRows("resolution", resCats.map, metric)} categories={resCats.cats} format={fmt(metric)} /></ChartCard>
          <ChartCard title={t("admin.stats.videoCodec")}><StackRows rows={stackRows("video_codec", codecCats.map, metric)} categories={codecCats.cats} format={fmt(metric)} /></ChartCard>
          <ChartCard title={t("admin.stats.hdr")}><StackRows rows={stackRows("hdr", hdrCats.map, metric)} categories={hdrCats.cats} format={fmt(metric)} /></ChartCard>
          <ChartCard title={t("admin.stats.audioCodec")}><StackRows rows={stackRows("audio_codec", acodecCats.map, metric)} categories={acodecCats.cats} format={fmt(metric)} /></ChartCard>
          <ChartCard title={t("admin.stats.channels")}><StackRows rows={stackRows("channels", chCats.map, metric)} categories={chCats.cats} format={fmt(metric)} /></ChartCard>
          <ChartCard title={t("admin.stats.container")}><StackRows rows={stackRows("container", contCats.map, metric)} categories={contCats.cats} format={fmt(metric)} /></ChartCard>
          <ChartCard title={t("admin.stats.match")} note={t("admin.stats.matchNote")}><StackRows rows={stackRows("match", matchCats.map, "files")} categories={matchCats.cats} format={(v) => num(v)} /></ChartCard>
        </div>
      </section>

      {/* Dettaglio */}
      <section className="sc-section">
        <div className="sc-section__head">
          <h2 className="sc-section__title">{t("admin.stats.detail")}</h2>
          <Segmented label={t("admin.stats.metric.label")} value={metric} onChange={setMetric} options={metricOpts} />
        </div>
        <div className="sc-filter" role="radiogroup" aria-label={t("admin.stats.col.library")}>
          <button type="button" role="radio" aria-checked={!lib} className={`chip${!lib ? " on" : ""}`} onClick={() => setLib("")}>{t("admin.stats.allLibraries")}</button>
          {libs.map((l) => <button key={l.id} type="button" role="radio" aria-checked={lib === l.id} className={`chip${lib === l.id ? " on" : ""}`} onClick={() => setLib(l.id)}>{l.name}</button>)}
        </div>

        <div className="sc-grid">
          <ChartCard title={t("admin.stats.growth")} note={t("admin.stats.growthNote")} wide>
            {growth.times.length > 1
              ? <LineChart title={t(`admin.stats.metric.${metric}` as MessageKey)} times={growth.times} series={[{ name: t("admin.stats.cumulative"), values: growth.cum, color: 1 }]} format={axisFmt(metric)} timeLabel={monthLabel} gapMs={Number.POSITIVE_INFINITY} integer={metric === "files"} />
              : <div className="t-small subtle">{t("admin.stats.noData")}</div>}
          </ChartCard>
          <ChartCard title={t("admin.stats.monthly")} note={metricNote} wide>
            <Columns bars={growth.bars} format={axisFmt(metric)} empty={t("admin.stats.noData")} />
          </ChartCard>
          <ChartCard title={t("admin.stats.decades")} note={metricNote}>
            <Columns bars={decades} format={axisFmt(metric)} empty={t("admin.stats.noData")} />
          </ChartCard>
          <ChartCard title={t("admin.stats.bitrate")} note={t("admin.stats.bitrateNote")}>
            <Columns bars={bitrateBars} format={(v) => num(v)} labelEvery={1} empty={t("admin.stats.noData")} />
          </ChartCard>
          <ChartCard title={t("admin.stats.audioLang")} note={t("admin.stats.langNote")}>
            <HBars rows={langRows("audio_language")} format={(v) => num(v)} total={selFiles} empty={t("admin.stats.noData")} />
          </ChartCard>
          <ChartCard title={t("admin.stats.subLang")} note={t("admin.stats.langNote")}>
            <HBars rows={langRows("subtitle_language")} format={(v) => num(v)} total={selFiles} empty={t("admin.stats.noData")} />
          </ChartCard>
          <ChartCard title={t("admin.stats.genres")} note={t("admin.stats.genresNote")}>
            <HBars rows={genreRows} format={(v) => num(v)} total={titles} empty={t("admin.stats.noData")} />
          </ChartCard>
          <ChartCard title={t("admin.stats.ratings")} note={t("admin.stats.ratingsNote")}>
            <Columns bars={ratingBars} format={(v) => num(v)} labelEvery={1} empty={t("admin.stats.noData")} />
          </ChartCard>
          <ChartCard title={t("admin.stats.heaviest")} note={t("admin.stats.heaviestNote")} wide>
            {heavy.length === 0 ? <div className="t-small subtle">{t("admin.stats.noData")}</div> : (
              <div className="sc-heavy">
                {heavy.map((h, i) => (
                  <Link key={h.id} to={`/item/${h.id}`} className="sc-heavy__row">
                    <span className="sc-heavy__rank">{i + 1}</span>
                    <span className="sc-heavy__title">{h.title}{h.year && !h.title.includes(`(${h.year})`) ? <span className="subtle"> ({h.year})</span> : null}</span>
                    <span className="sc-heavy__size">{bytes(h.size)}</span>
                    <span className="sc-heavy__meta">
                      <span>{libName.get(h.library)}</span>
                      <span>{h.kind === "series" ? t("admin.stats.episodesShort", { count: h.files }) : t("admin.libraries.files", { count: h.files })}</span>
                      {h.duration_ms > 0 && <span>{longDuration(h.duration_ms)}</span>}
                      <span className="sc-heavy__bar"><span style={{ width: `${(h.size / heavy[0].size) * 100}%` }} /></span>
                    </span>
                  </Link>
                ))}
              </div>
            )}
          </ChartCard>
          <ChartCard title={t("admin.stats.health")} wide>
            <div className="sc-health">
              <Health label={t("admin.stats.h.missing")} value={num(s((l) => l.missing))} />
              <Health label={t("admin.stats.h.probeFailed")} value={num(s((l) => l.probe_failed))} />
              <Health label={t("admin.stats.h.probePending")} value={num(s((l) => l.probe_pending))} />
              <Health label={t("admin.stats.h.unknownRes")} value={num(unknownRes)} />
              <Health label={t("admin.stats.h.versions")} value={num(s((l) => l.multi_version_items))} sub={t("admin.stats.h.versionsSub", { size: bytes(s((l) => l.multi_version_extra_size)) })} />
              <Health label={t("admin.stats.h.extras")} value={num(s((l) => l.extras))} sub={bytes(s((l) => l.extras_size))} />
              <Health label={t("admin.stats.h.strm")} value={num(s((l) => l.strm))} />
              <Health label={t("admin.stats.h.collections")} value={num(s((l) => l.collections))} />
            </div>
          </ChartCard>
        </div>
      </section>
    </div>
  );
}

function Health({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="sc-health__item">
      <span className="t-small muted">{label}</span>
      <span className="sc-health__value">{value}</span>
      {sub && <span className="t-small subtle">{sub}</span>}
    </div>
  );
}
