/**
 * Interfaccia del player degli audiolibri: barra in basso (sempre visibile mentre c'è un libro) e vista completa
 * con copertina, capitoli, velocità e timer di spegnimento.
 */
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { ChevronDown, Gauge, ListOrdered, LoaderCircle, Moon, Pause, Play, RotateCcw, RotateCw, SkipBack, SkipForward, X } from "lucide-react";
import { imageUrl } from "../api/client";
import { useI18n } from "../i18n";
import { chapterBounds, useAudio, type Book } from "./AudioPlayer";

const RATES = [0.75, 0.9, 1, 1.1, 1.2, 1.25, 1.3, 1.5, 1.75, 2, 2.5, 3];
const SLEEP_MIN = [5, 10, 15, 30, 45, 60, 90];
const SKIPS = [10, 15, 30, 45, 60];

function Cover({ book, size }: { book: Book; size: number }) {
  return (
    <div className="ab-cover" style={{ background: book.color ?? undefined }}>
      {book.cover && <img src={imageUrl(book.cover, size)} alt="" />}
    </div>
  );
}

/** Pulsante di salto con il numero dei secondi dentro la freccia. */
function SkipButton({ seconds, back, onClick, big }: { seconds: number; back: boolean; onClick: () => void; big?: boolean }) {
  const { t } = useI18n();
  const label = t(back ? "player.backN" : "player.forwardN", { s: seconds });
  return (
    <button type="button" className={`ab-skip${big ? " ab-skip--big" : ""}`} onClick={onClick} aria-label={label} title={label}>
      {back ? <RotateCcw /> : <RotateCw />}
      <span>{seconds}</span>
    </button>
  );
}

function PlayButton({ big }: { big?: boolean }) {
  const { t } = useI18n();
  const { playing, loading, toggle } = useAudio();
  return (
    <button type="button" className={`ab-play${big ? " ab-play--big" : ""}`} onClick={toggle} aria-label={playing ? t("player.pause") : t("player.play")}>
      {loading && playing ? <LoaderCircle className="ab-spin" /> : playing ? <Pause fill="currentColor" /> : <Play fill="currentColor" />}
    </button>
  );
}

function rateLabel(r: number, num: (n: number, d?: number) => string) {
  return `${num(r, Math.abs(r * 10 - Math.round(r * 10)) < 1e-6 ? 1 : 2)}×`;
}

/** Barra in basso: copertina, titolo, capitolo, comandi essenziali. Toccandola si apre la vista completa. */
export function AudioBar() {
  const { t } = useI18n();
  const a = useAudio();
  const { book } = a;
  useEffect(() => {
    document.documentElement.classList.toggle("has-audio-bar", !!book);
    return () => document.documentElement.classList.remove("has-audio-bar");
  }, [book]);
  if (!book) return null;
  const ch = a.chapter >= 0 ? book.chapters[a.chapter] : null;
  const progress = book.duration ? Math.min(1, a.position / book.duration) : 0;
  return (
    <>
      <div className="ab-bar" role="region" aria-label={t("audio.nowPlaying")}>
        <div className="ab-bar__progress"><span style={{ width: `${progress * 100}%` }} /></div>
        <button type="button" className="ab-bar__open" onClick={() => a.setExpanded(true)} aria-label={t("audio.open")}>
          <Cover book={book} size={96} />
          <span className="ab-bar__text">
            <span className="ab-bar__title">{book.title}</span>
            <span className="ab-bar__sub">{ch ? ch.title ?? t("player.chapterN", { n: a.chapter + 1 }) : book.authors ?? ""}</span>
          </span>
        </button>
        <div className="ab-bar__controls">
          <span className="ab-hide-sm"><SkipButton seconds={a.skipBack} back onClick={() => a.skip(-a.skipBack)} /></span>
          <PlayButton />
          <SkipButton seconds={a.skipForward} back={false} onClick={() => a.skip(a.skipForward)} />
          <button type="button" className="icon-btn ab-hide-sm" onClick={a.close} aria-label={t("audio.close")} title={t("audio.close")}><X /></button>
        </div>
      </div>
      {a.expanded && <FullPlayer />}
    </>
  );
}

type Panel = "chapters" | "speed" | "sleep" | null;

function FullPlayer() {
  const { t, clock, duration, num } = useI18n();
  const a = useAudio();
  const book = a.book!;
  const [panel, setPanel] = useState<Panel>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const [scope, setScope] = useState<"chapter" | "book">("chapter");
  const sheet = useRef<HTMLDivElement>(null);
  const hasChapters = book.chapters.length > 1;
  const [cStart, cEnd] = scope === "chapter" && hasChapters ? chapterBounds(book, a.chapter) : [0, book.duration];
  const span = Math.max(1, cEnd - cStart);
  const pos = drag ?? a.position;
  const inScope = Math.max(0, Math.min(span, pos - cStart));
  const ch = a.chapter >= 0 ? book.chapters[a.chapter] : null;
  const left = Math.max(0, book.duration - a.position);

  // Esc chiude; spazio e frecce comandano la riproduzione (fuori dai campi di testo).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (el && (el.tagName === "INPUT" && (el as HTMLInputElement).type !== "range") || el?.tagName === "TEXTAREA" || el?.tagName === "SELECT") return;
      if (e.key === "Escape") a.setExpanded(false);
      else if (e.key === " " && el?.tagName !== "BUTTON") { e.preventDefault(); a.toggle(); }
      else if (e.key === "ArrowLeft" && el?.tagName !== "INPUT") { e.preventDefault(); if (e.shiftKey) a.chapterJump(-1); else a.skip(-a.skipBack); }
      else if (e.key === "ArrowRight" && el?.tagName !== "INPUT") { e.preventDefault(); if (e.shiftKey) a.chapterJump(1); else a.skip(a.skipForward); }
    };
    window.addEventListener("keydown", onKey);
    document.documentElement.classList.add("ab-open");
    sheet.current?.focus();
    return () => { window.removeEventListener("keydown", onKey); document.documentElement.classList.remove("ab-open"); };
  }, [a]);

  // Capitolo corrente in vista quando si apre l'elenco.
  // Su telefono il pannello sta sotto i comandi: si porta in vista quando si apre.
  useEffect(() => {
    if (!panel) return;
    const p = document.querySelector(".ab-sheet .ab-panel");
    if (p && window.matchMedia("(max-width: 960px)").matches) p.scrollIntoView({ block: "start", behavior: "smooth" });
    if (panel === "chapters") document.querySelector(".ab-sheet .ab-chapters .is-current")?.scrollIntoView({ block: "center" });
  }, [panel]);

  const sleepLabel = a.sleep?.kind === "time" ? clock(a.sleep.remaining) : a.sleep?.kind === "chapter" ? t("audio.sleepChapterShort") : t("audio.sleep");
  const toggle = (p: Panel) => setPanel((cur) => (cur === p ? null : p));

  return (
    <div className="ab-sheet" role="dialog" aria-modal="true" aria-label={t("audio.nowPlaying")} tabIndex={-1} ref={sheet}>
      <div className="ab-sheet__head">
        <button type="button" className="icon-btn" onClick={() => a.setExpanded(false)} aria-label={t("audio.collapse")} title={t("audio.collapse")}><ChevronDown /></button>
        <span className="pl-eyebrow">{t("audio.nowPlaying")}</span>
        <button type="button" className="icon-btn" onClick={a.close} aria-label={t("audio.close")} title={t("audio.close")}><X /></button>
      </div>
      <div className={`ab-sheet__body${panel ? " has-panel" : ""}`}>
        <div className="ab-main">
          <Cover book={book} size={720} />
          <div className="ab-titles">
            <Link to={`/item/${book.id}`} className="ab-titles__book t-h2" onClick={() => a.setExpanded(false)}>{book.title}</Link>
            {book.authors && <div className="muted">{book.authors}</div>}
            {book.narrators.length > 0 && <div className="t-small subtle">{t("audio.readBy", { names: book.narrators.join(", ") })}</div>}
          </div>
          {ch && (
            <button type="button" className="ab-chapter-btn" onClick={() => toggle("chapters")}>
              <span className="t-label">{ch.title ?? t("player.chapterN", { n: a.chapter + 1 })}</span>
              <span className="t-small subtle">{t("audio.chapterOf", { n: a.chapter + 1, total: book.chapters.length })}</span>
            </button>
          )}
          <div className="ab-seek">
            <input type="range" className="ab-range" min={0} max={span} step={1000} value={inScope} aria-label={t("player.position")}
              aria-valuetext={`${clock(inScope)} / ${clock(span)}`}
              style={{ "--p": `${(inScope / span) * 100}%` } as React.CSSProperties}
              onChange={(e) => setDrag(cStart + Number(e.target.value))}
              onPointerUp={() => { if (drag != null) { a.seek(drag); setDrag(null); } }}
              onKeyUp={() => { if (drag != null) { a.seek(drag); setDrag(null); } }}
              onBlur={() => setDrag(null)} />
            <div className="ab-seek__times t-mono-sm">
              <span>{clock(inScope)}</span>
              {hasChapters ? (
                <button type="button" className="ab-scope" onClick={() => setScope(scope === "chapter" ? "book" : "chapter")} title={t("audio.scopeHint")}>
                  {scope === "chapter" ? t("audio.scopeChapter") : t("audio.scopeBook")}
                </button>
              ) : <span />}
              <span>−{clock(Math.max(0, span - inScope) / a.rate)}</span>
            </div>
          </div>
          <div className="ab-transport">
            <button type="button" className="icon-btn" onClick={() => a.chapterJump(-1)} aria-label={t("audio.prevChapter")} title={t("audio.prevChapter")}><SkipBack /></button>
            <SkipButton big seconds={a.skipBack} back onClick={() => a.skip(-a.skipBack)} />
            <PlayButton big />
            <SkipButton big seconds={a.skipForward} back={false} onClick={() => a.skip(a.skipForward)} />
            <button type="button" className="icon-btn" onClick={() => a.chapterJump(1)} aria-label={t("audio.nextChapter")} title={t("audio.nextChapter")} disabled={hasChapters && a.chapter >= book.chapters.length - 1}><SkipForward /></button>
          </div>
          <div className="t-small subtle ab-left">
            {t("audio.leftInBook", { time: duration(left / a.rate), percent: num(book.duration ? (a.position / book.duration) * 100 : 0, 0) })}
          </div>
          {a.error && <div className="t-small danger-text" role="alert">{a.error}</div>}
          <div className="ab-tools">
            <button type="button" className={`ab-tool${panel === "speed" ? " on" : ""}`} onClick={() => toggle("speed")} aria-expanded={panel === "speed"}><Gauge /><span>{rateLabel(a.rate, num)}</span></button>
            <button type="button" className={`ab-tool${panel === "sleep" || a.sleep ? " on" : ""}`} onClick={() => toggle("sleep")} aria-expanded={panel === "sleep"}><Moon /><span>{sleepLabel}</span></button>
            {hasChapters && <button type="button" className={`ab-tool${panel === "chapters" ? " on" : ""}`} onClick={() => toggle("chapters")} aria-expanded={panel === "chapters"}><ListOrdered /><span>{t("player.chapters")}</span></button>}
          </div>
        </div>
        {panel && (
          <div className="ab-panel">
            {panel === "speed" && (
              <div className="stack">
                <div className="t-h3">{t("audio.speed")}</div>
                <div className="ab-rate">
                  <button type="button" className="icon-btn" onClick={() => a.setRate(a.rate - 0.05)} aria-label={t("audio.slower")}>−</button>
                  <span className="t-numeral">{rateLabel(a.rate, num)}</span>
                  <button type="button" className="icon-btn" onClick={() => a.setRate(a.rate + 0.05)} aria-label={t("audio.faster")}>+</button>
                </div>
                <div className="ab-chips">
                  {RATES.map((r) => <button key={r} type="button" className={`ab-chip${Math.abs(a.rate - r) < 0.001 ? " on" : ""}`} onClick={() => a.setRate(r)}>{rateLabel(r, num)}</button>)}
                </div>
                <div className="t-h3" style={{ marginTop: 8 }}>{t("audio.skipTitle")}</div>
                <div className="ab-skips">
                  <span className="t-small subtle">{t("audio.skipBack")}</span>
                  <div className="ab-chips">{SKIPS.map((s) => <button key={s} type="button" className={`ab-chip${a.skipBack === s ? " on" : ""}`} onClick={() => a.setSkips(s, a.skipForward)}>{s} s</button>)}</div>
                  <span className="t-small subtle">{t("audio.skipForward")}</span>
                  <div className="ab-chips">{SKIPS.map((s) => <button key={s} type="button" className={`ab-chip${a.skipForward === s ? " on" : ""}`} onClick={() => a.setSkips(a.skipBack, s)}>{s} s</button>)}</div>
                </div>
              </div>
            )}
            {panel === "sleep" && (
              <div className="stack">
                <div className="t-h3">{t("audio.sleepTitle")}</div>
                <div className="t-small subtle">{t("audio.sleepHint")}</div>
                <div className="ab-chips">
                  <button type="button" className={`ab-chip${!a.sleep ? " on" : ""}`} onClick={() => a.setSleep(null)}>{t("audio.sleepOff")}</button>
                  {SLEEP_MIN.map((m) => <button key={m} type="button" className="ab-chip" onClick={() => { a.setSleep({ kind: "time", remaining: m * 60_000 }); setPanel(null); }}>{t("audio.minutes", { n: m })}</button>)}
                  {hasChapters && <button type="button" className={`ab-chip${a.sleep?.kind === "chapter" ? " on" : ""}`} onClick={() => { a.setSleep({ kind: "chapter" }); setPanel(null); }}>{t("audio.sleepChapter")}</button>}
                </div>
                {a.sleep?.kind === "time" && <div className="t-small">{t("audio.sleepLeft", { time: clock(a.sleep.remaining) })}</div>}
              </div>
            )}
            {panel === "chapters" && (
              <ol className="ab-chapters">
                {book.chapters.map((c, i) => {
                  const end = i + 1 < book.chapters.length ? book.chapters[i + 1].start_ms : book.duration;
                  return (
                    <li key={i} className={i === a.chapter ? "is-current" : i < a.chapter ? "is-past" : undefined}>
                      <button type="button" onClick={() => a.seek(c.start_ms)} aria-current={i === a.chapter ? "true" : undefined}>
                        <span className="ab-chapters__n t-mono-sm">{i + 1}</span>
                        <span className="ab-chapters__title">{c.title ?? t("player.chapterN", { n: i + 1 })}</span>
                        <span className="t-mono-sm subtle">{clock(end - c.start_ms)}</span>
                      </button>
                    </li>
                  );
                })}
              </ol>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
