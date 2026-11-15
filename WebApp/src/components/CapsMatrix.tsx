import { useI18n } from "../i18n";

export interface CapsResult { backend: string; kind: string; codec: string; ok: boolean; ms: number; error: string | null }
export interface Capabilities {
  tested_at: number;
  devices: string[];
  software_encoders: string[];
  selected: { backend: string; device: string; encode: string[]; decode: string[] } | null;
  results: CapsResult[];
}

/** Matrice delle prove reali di codifica e decodifica per backend (server o nodo). */
export function CapsMatrix({ caps }: { caps: Capabilities }) {
  const { t } = useI18n();
  const backends = Array.from(new Set(caps.results.map((r) => r.backend)));
  const codecs = Array.from(new Set(caps.results.map((r) => `${r.kind}:${r.codec}`)));
  if (!caps.results.length) return <p className="t-small muted" style={{ margin: 0 }}>{t("admin.nodes.noTest")}</p>;
  return (
    <div className="table-wrap">
      <table className="table matrix">
        <thead><tr><th>{t("admin.transcode.test")}</th>{backends.map((b) => <th key={b}>{b.toUpperCase()}</th>)}</tr></thead>
        <tbody>
          {codecs.map((k) => {
            const [kind, codec] = k.split(":");
            return (
              <tr key={k}>
                <td className="t-label">{kind === "encode" ? t("admin.transcode.encode") : t("admin.transcode.decode")} <span className="t-mono">{codec.toUpperCase().replace("10", " 10 bit")}</span></td>
                {backends.map((b) => {
                  const r = caps.results.find((x) => x.backend === b && x.kind === kind && x.codec === codec);
                  return <td key={b} className={`t-mono-sm ${r?.ok ? "ok" : "ko"}`} title={r?.error ?? undefined}>{r ? (r.ok ? `✓ ${r.ms} ms` : "✗") : "—"}</td>;
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
