import { useEffect, useRef, useState } from "react";

/** Una serie: nome (legenda), valori allineati a `times`, colore per ruolo (`--chart-1`, `--chart-2`). */
export interface Series { name: string; values: number[]; color: 1 | 2 }

interface Props {
  title: string;
  times: number[];
  series: Series[];
  /** Testo di un valore (asse, legenda, suggerimento). */
  format: (v: number) => string;
  /** Massimo fisso dell'asse (es. 100 per le percentuali); altrimenti dal dato. */
  max?: number;
  /** Ora del giorno per l'asse e i suggerimenti. */
  timeLabel: (t: number) => string;
  /** Gli intervalli più lunghi di così sono buchi (server spento): la linea si interrompe. */
  gapMs: number;
  /** Valori interi (asse senza decimali). */
  integer?: boolean;
}

const H = 168;
const PAD = { top: 8, right: 8, bottom: 22, left: 52 };

/** Massimo "tondo" dell'asse e 4 tacche. */
function niceMax(v: number, integer?: boolean): number {
  if (v <= 0) return integer ? 4 : 1;
  const exp = Math.pow(10, Math.floor(Math.log10(v)));
  for (const m of [1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]) {
    const c = m * exp;
    if (c >= v) return integer ? Math.max(4, Math.ceil(c / 4) * 4) : c;
  }
  return 10 * exp;
}

export function LineChart({ title, times, series, format, max, timeLabel, gapMs, integer }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(600);
  const [hover, setHover] = useState<number | null>(null);
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setW(Math.max(240, Math.floor(e.contentRect.width))));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const n = times.length;
  const top = max ?? niceMax(Math.max(0, ...series.flatMap((s) => s.values)), integer);
  const iw = w - PAD.left - PAD.right;
  const ih = H - PAD.top - PAD.bottom;
  const t0 = n ? times[0] : 0;
  const t1 = n ? times[n - 1] : 1;
  const x = (t: number) => PAD.left + (t1 > t0 ? ((t - t0) / (t1 - t0)) * iw : iw);
  const y = (v: number) => PAD.top + ih - (Math.min(v, top) / top) * ih;
  const path = (vals: number[]) => {
    let d = "";
    for (let i = 0; i < n; i++) {
      const gap = i === 0 || times[i] - times[i - 1] > gapMs;
      d += `${gap ? "M" : "L"}${x(times[i]).toFixed(1)},${y(vals[i]).toFixed(1)}`;
    }
    return d;
  };
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => f * top);
  // Tacche dell'ora senza doppioni (con poco storico due tacche cadono nello stesso minuto).
  const xticks = (n > 1 ? [0, 0.25, 0.5, 0.75, 1].map((f) => t0 + f * (t1 - t0)) : []).filter((t, i, a) => i === 0 || timeLabel(t) !== timeLabel(a[i - 1]));

  const onMove = (e: React.PointerEvent<SVGRectElement>) => {
    if (!n) return;
    const r = e.currentTarget.getBoundingClientRect();
    const t = t0 + ((e.clientX - r.left) / r.width) * (t1 - t0);
    // Campione più vicino (i tempi sono ordinati).
    let lo = 0, hi = n - 1;
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1;
      if (times[mid] < t) lo = mid; else hi = mid;
    }
    setHover(Math.abs(times[lo] - t) <= Math.abs(times[hi] - t) ? lo : hi);
  };
  const last = n - 1;
  const shown = hover ?? last;
  const tipLeft = hover != null ? Math.min(Math.max(x(times[hover]) - 80, 0), w - 168) : 0;

  return (
    <figure className="chart" aria-label={`${title}: ${series.map((s) => `${s.name} ${n ? format(s.values[last]) : "—"}`).join(", ")}`}>
      <figcaption className="chart__head">
        <span className="chart__title">{title}</span>
        <span className="chart__legend">
          {series.map((s) => (
            <span key={s.name} className="chart__key">
              <span className={`chart__swatch chart__swatch--${s.color}`} aria-hidden />
              {s.name} <strong>{n ? format(s.values[shown]) : "—"}</strong>
            </span>
          ))}
        </span>
      </figcaption>
      <div ref={wrap} className="chart__plot">
        <svg width={w} height={H} role="img" aria-hidden>
          {ticks.map((v, i) => (
            <g key={i}>
              <line className="chart__grid" x1={PAD.left} x2={w - PAD.right} y1={y(v)} y2={y(v)} />
              <text className="chart__axis" x={PAD.left - 6} y={y(v)} dy="0.32em" textAnchor="end">{format(v)}</text>
            </g>
          ))}
          {xticks.map((t, i) => (
            <text key={i} className="chart__axis" x={x(t)} y={H - 6} textAnchor={i === 0 ? "start" : i === xticks.length - 1 ? "end" : "middle"}>{timeLabel(t)}</text>
          ))}
          {series.map((s) => <path key={s.name} className={`chart__line chart__line--${s.color}`} d={path(s.values)} />)}
          {hover != null && (
            <g>
              <line className="chart__cross" x1={x(times[hover])} x2={x(times[hover])} y1={PAD.top} y2={PAD.top + ih} />
              {series.map((s) => <circle key={s.name} className={`chart__dot chart__dot--${s.color}`} cx={x(times[hover])} cy={y(s.values[hover])} r={4} />)}
            </g>
          )}
          <rect x={PAD.left} y={PAD.top} width={iw} height={ih} fill="transparent" onPointerMove={onMove} onPointerLeave={() => setHover(null)} />
        </svg>
        {hover != null && (
          <div className="chart__tip" style={{ left: tipLeft }}>
            <div className="chart__tip-time">{timeLabel(times[hover])}</div>
            {series.map((s) => (
              <div key={s.name} className="chart__tip-row"><span className={`chart__swatch chart__swatch--${s.color}`} aria-hidden />{s.name}<strong>{format(s.values[hover])}</strong></div>
            ))}
          </div>
        )}
      </div>
    </figure>
  );
}
