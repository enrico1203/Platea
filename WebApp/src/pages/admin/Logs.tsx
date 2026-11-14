import { useEffect, useRef, useState } from "react";
import { api } from "../../api/client";
import { subscribe } from "../../api/events";
import { SelectField } from "../../components/ui";
import { useI18n } from "../../i18n";

interface Line { seq: number; at: number; level: string; message: string }
const RANK: Record<string, number> = { ERROR: 4, WARN: 3, INFO: 2, DEBUG: 1 };
const COLOR: Record<string, string> = { ERROR: "var(--danger)", WARN: "var(--warning)", INFO: "var(--ink-muted)", DEBUG: "var(--ink-subtle)" };

/** Log in tempo reale: le righe nuove arrivano sul WebSocket degli eventi, nessuna interrogazione periodica. */
export default function Logs() {
  const { t } = useI18n();
  const [level, setLevel] = useState("INFO");
  const [lines, setLines] = useState<Line[]>([]);
  const [follow, setFollow] = useState(true);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    api<{ items: Line[] }>(`/api/v1/admin/logs?level=${level}&limit=500`).then((r) => setLines(r.items));
    return subscribe((e) => {
      if (e.type !== "log.line" || !e.data) return;
      const l = e.data as unknown as Line;
      if ((RANK[l.level] ?? 0) < (RANK[level] ?? 0)) return;
      setLines((prev) => (prev.length > 1500 ? [...prev.slice(-1000), l] : [...prev, l]));
    });
  }, [level]);
  useEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [lines, follow]);
  const time = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour12: false });
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <h1 className="t-h1">{t("admin.logs.title")}</h1>
        <SelectField label={t("admin.logs.level")} value={level} onChange={setLevel} options={["ERROR", "WARN", "INFO", "DEBUG"].map((l) => ({ value: l, label: l }))} />
      </div>
      <div
        ref={box}
        className="pl-panel t-mono"
        style={{ height: "70vh", overflowY: "auto", padding: 16, fontSize: 12, lineHeight: "18px" }}
        onScroll={(e) => { const el = e.currentTarget; setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 40); }}
      >
        {lines.length === 0 && <div className="subtle">{t("admin.logs.empty")}</div>}
        {lines.map((l) => (
          <div key={l.seq} style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>
            <span className="subtle">{time(l.at)} </span>
            <span style={{ color: COLOR[l.level] }}>{l.level.padEnd(5)} </span>
            <span>{l.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
