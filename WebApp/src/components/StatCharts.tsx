import { useEffect, useRef, useState, type ReactNode } from "react";

/**
 * Grafici delle statistiche: barre orizzontali (una tinta), righe impilate al 100% (composizione per categoria) e
 * colonne. Colori per ruolo: `--chart-1`…`--chart-8` in ordine fisso, `--chart-other` per "altro/sconosciuto".
 * Testi sempre con i colori del testo; il colore sta solo sui segni.
 */

export type Slot = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | "other";
export interface Category { key: string; label: string; slot: Slot }

const color = (s: Slot) => (s === "other" ? "var(--chart-other)" : `var(--chart-${s})`);

/** Suggerimento al passaggio del puntatore, posizionato dentro il contenitore (che deve essere `position: relative`). */
interface TipState { x: number; y: number; body: ReactNode }
function useTip() {
  const box = useRef<HTMLDivElement>(null);
  const [tip, setTip] = useState<TipState | null>(null);
  const show = (e: React.PointerEvent, body: ReactNode) => {
    const r = box.current?.getBoundingClientRect();
    if (!r) return;
    setTip({ x: e.clientX - r.left, y: e.clientY - r.top, body });
  };
  const hide = () => setTip(null);
  const width = box.current?.clientWidth ?? 0;
  const node = tip ? (
    <div className="sc-tip" role="status" style={{ left: Math.max(0, Math.min(tip.x + 12, width - 200)), top: tip.y + 14 }}>{tip.body}</div>
  ) : null;
  return { box, show, hide, node };
}

function TipRow({ slot, label, value, sub }: { slot?: Slot; label: ReactNode; value: ReactNode; sub?: ReactNode }) {
  return (
    <div className="sc-tip__row">
      {slot != null && <span className="sc-swatch" style={{ background: color(slot) }} aria-hidden />}
      <span>{label}</span>
      <strong>{value}</strong>
      {sub != null && <span className="sc-tip__sub">{sub}</span>}
    </div>
  );
}

export function Legend({ categories }: { categories: Category[] }) {
  return (
    <div className="sc-legend">
      {categories.map((c) => (
        <span key={c.key} className="sc-legend__key"><span className="sc-swatch" style={{ background: color(c.slot) }} aria-hidden />{c.label}</span>
      ))}
    </div>
  );
}

const pct = (v: number, tot: number) => (tot > 0 ? (v / tot) * 100 : 0);
/** Quota in percentuale, nella lingua della pagina. */
const pctText = (p: number) => {
  const f = (d: number, v: number) => new Intl.NumberFormat(document.documentElement.lang || undefined, { maximumFractionDigits: d, minimumFractionDigits: d }).format(v);
  return p > 0 && p < 0.1 ? `<${f(1, 0.1)}%` : `${f(p < 10 ? 1 : 0, p)}%`;
};
export { pctText };

/** Barre orizzontali: una per riga, valore e quota scritti accanto. */
export function HBars({ rows, format, total, empty }: {
  rows: { key: string; label: ReactNode; value: number; note?: ReactNode }[];
  format: (v: number) => string;
  /** Totale per la quota (%); senza, la somma delle righe. */
  total?: number;
  empty?: ReactNode;
}) {
  const { box, show, hide, node } = useTip();
  const max = Math.max(0, ...rows.map((r) => r.value));
  const tot = total ?? rows.reduce((a, r) => a + r.value, 0);
  if (!rows.length || max <= 0) return <div className="t-small subtle">{empty}</div>;
  return (
    <div className="sc-hbars" ref={box} onPointerLeave={hide}>
      {rows.map((r) => {
        const p = pct(r.value, tot);
        return (
          <div key={r.key} className="sc-hbar" onPointerMove={(e) => show(e, <TipRow slot={1} label={r.label} value={format(r.value)} sub={pctText(p)} />)}>
            <div className="sc-hbar__label">{r.label}{r.note != null && <span className="sc-hbar__note">{r.note}</span>}</div>
            <div className="sc-hbar__track"><div className="sc-hbar__fill" style={{ width: `${Math.max(0.4, (r.value / max) * 100)}%` }} /></div>
            <div className="sc-hbar__value">{format(r.value)} <span className="sc-hbar__pct">{pctText(p)}</span></div>
          </div>
        );
      })}
      {node}
    </div>
  );
}

/** Composizione: una barra al 100% per riga (es. libreria), segmenti per categoria. */
export function StackRows({ rows, categories, format, empty }: {
  rows: { key: string; label: ReactNode; values: Record<string, number>; strong?: boolean }[];
  categories: Category[];
  format: (v: number) => string;
  empty?: ReactNode;
}) {
  const { box, show, hide, node } = useTip();
  const visible = rows.filter((r) => categories.some((c) => (r.values[c.key] ?? 0) > 0));
  if (!visible.length) return <div className="t-small subtle">{empty}</div>;
  return (
    <div className="sc-stack" ref={box} onPointerLeave={hide}>
      <Legend categories={categories} />
      {visible.map((r) => {
        const tot = categories.reduce((a, c) => a + (r.values[c.key] ?? 0), 0);
        const segs = categories.filter((c) => (r.values[c.key] ?? 0) > 0);
        const aria = segs.map((c) => `${c.label} ${pctText(pct(r.values[c.key], tot))}`).join(", ");
        return (
          <div key={r.key} className={`sc-stack__row${r.strong ? " sc-stack__row--total" : ""}`}>
            <div className="sc-stack__head"><span className="sc-stack__label">{r.label}</span><span className="sc-stack__total">{format(tot)}</span></div>
            <div className="sc-stack__bar" role="img" aria-label={aria}>
              {segs.map((c) => {
                const v = r.values[c.key];
                const p = pct(v, tot);
                return (
                  <div key={c.key} className="sc-stack__seg" style={{ flexGrow: v, background: color(c.slot) }}
                    onPointerMove={(e) => show(e, <><div className="sc-tip__title">{r.label}</div><TipRow slot={c.slot} label={c.label} value={format(v)} sub={pctText(p)} /></>)} />
                );
              })}
            </div>
            <div className="sc-stack__labels" aria-hidden>
              {segs.filter((c) => pct(r.values[c.key], tot) >= 8).slice(0, 4).map((c) => (
                <span key={c.key}><span className="sc-swatch" style={{ background: color(c.slot) }} />{c.label} {pctText(pct(r.values[c.key], tot))}</span>
              ))}
            </div>
          </div>
        );
      })}
      {node}
    </div>
  );
}

const H = 180;
const PAD = { top: 10, right: 4, bottom: 24, left: 56 };

function niceMax(v: number): number {
  if (v <= 0) return 1;
  const exp = Math.pow(10, Math.floor(Math.log10(v)));
  for (const m of [1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]) if (m * exp >= v) return m * exp;
  return 10 * exp;
}

/** Colonne verticali (istogrammi, valori per mese o per decennio). */
export function Columns({ bars, format, labelEvery, empty }: {
  bars: { key: string; label: string; tip?: string; value: number }[];
  format: (v: number) => string;
  /** Scrive un'etichetta sull'asse ogni N colonne (le altre restano nel suggerimento). */
  labelEvery?: number;
  empty?: ReactNode;
}) {
  const { box, show, hide, node } = useTip();
  const [w, setW] = useState(600);
  const [hover, setHover] = useState<number | null>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setW(Math.max(240, Math.floor(e.contentRect.width))));
    ro.observe(el);
    return () => ro.disconnect();
  }, [box]);
  const n = bars.length;
  const top = niceMax(Math.max(0, ...bars.map((b) => b.value)));
  if (!n || top <= 0) return <div className="t-small subtle">{empty}</div>;
  const iw = w - PAD.left - PAD.right;
  const ih = H - PAD.top - PAD.bottom;
  const step = iw / n;
  const gap = Math.min(step * 0.3, 8);
  const bw = Math.max(1, step - Math.max(gap, step > 4 ? 2 : 0));
  const y = (v: number) => PAD.top + ih - (v / top) * ih;
  const every = labelEvery ?? Math.max(1, Math.ceil(n / Math.max(2, Math.floor(iw / 56))));
  const r = Math.min(4, bw / 2);
  return (
    <div className="sc-cols" ref={box} onPointerLeave={() => { hide(); setHover(null); }}>
      <svg width={w} height={H} aria-hidden>
        {[0, 0.25, 0.5, 0.75, 1].map((f) => (
          <g key={f}>
            <line className="chart__grid" x1={PAD.left} x2={w - PAD.right} y1={y(f * top)} y2={y(f * top)} />
            <text className="chart__axis" x={PAD.left - 6} y={y(f * top)} dy="0.32em" textAnchor="end">{format(f * top)}</text>
          </g>
        ))}
        {bars.map((b, i) => {
          const x = PAD.left + i * step + (step - bw) / 2;
          const h = Math.max(b.value > 0 ? 1 : 0, (b.value / top) * ih);
          const yy = PAD.top + ih - h;
          const rr = Math.min(r, h);
          // Estremità arrotondata solo in alto: la base resta appoggiata all'asse.
          const d = `M${x},${PAD.top + ih}V${yy + rr}Q${x},${yy} ${x + rr},${yy}H${x + bw - rr}Q${x + bw},${yy} ${x + bw},${yy + rr}V${PAD.top + ih}Z`;
          return <path key={b.key} d={d} className={`sc-col${hover === i ? " sc-col--hover" : ""}`} />;
        })}
        {bars.map((b, i) => (i % every === 0 ? (
          <text key={b.key} className="chart__axis" x={PAD.left + i * step + step / 2} y={H - 6} textAnchor="middle">{b.label}</text>
        ) : null))}
        {bars.map((b, i) => (
          <rect key={b.key} x={PAD.left + i * step} y={PAD.top} width={step} height={ih} fill="transparent"
            onPointerMove={(e) => { setHover(i); show(e, <TipRow label={b.tip ?? b.label} value={format(b.value)} />); }} />
        ))}
      </svg>
      {node}
    </div>
  );
}

/** Contenitore di un grafico con titolo, nota e controlli. */
export function ChartCard({ title, note, actions, children, wide }: { title: ReactNode; note?: ReactNode; actions?: ReactNode; children: ReactNode; wide?: boolean }) {
  return (
    <section className={`pl-panel panel-pad sc-card${wide ? " sc-card--wide" : ""}`}>
      <header className="sc-card__head">
        <div>
          <h2 className="sc-card__title">{title}</h2>
          {note && <div className="t-small subtle">{note}</div>}
        </div>
        {actions}
      </header>
      {children}
    </section>
  );
}

/** Scelta tra poche opzioni (misura, libreria). */
export function Segmented<T extends string>({ value, onChange, options, label }: { value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode }[]; label: string }) {
  return (
    <div className="sc-seg" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={value === o.value} className={`sc-seg__opt${value === o.value ? " on" : ""}`} onClick={() => onChange(o.value)}>{o.label}</button>
      ))}
    </div>
  );
}
