import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FlaskConical } from "lucide-react";
import { api } from "../../api/client";
import { Alert } from "../../components/ui";
import { CapsMatrix, type Capabilities } from "../../components/CapsMatrix";
import { useI18n } from "../../i18n";

interface Caps {
  ffmpeg: { ffmpeg: string; ffprobe: string; version: string; is_jellyfin: boolean; available: boolean };
  capabilities: Capabilities;
  current: { backend: string };
  slots: { hw_used: number; hw_max: number; sw_used: number; sw_max: number };
}

export default function Transcode() {
  const { t, relative } = useI18n();
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const { data } = useQuery({ queryKey: ["admin", "transcode"], queryFn: () => api<Caps>("/api/v1/admin/transcode") });
  if (!data) return null;
  const c = data.capabilities;
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <h1 className="t-h1">{t("admin.transcode.title")}</h1>
        <button className="pl-btn pl-btn--secondary" disabled={busy} onClick={async () => { setBusy(true); try { qc.setQueryData(["admin", "transcode"], await api<Caps>("/api/v1/admin/transcode/test", { method: "POST" })); } finally { setBusy(false); } }}><FlaskConical />{busy ? t("setup.testing") : t("admin.transcode.testNow")}</button>
      </div>
      {!data.ffmpeg.available && <Alert tone="danger">{t("admin.transcode.ffmpegMissing")}</Alert>}
      <section className="pl-panel panel-pad">
        <dl className="kv">
          <dt>FFmpeg</dt><dd className="t-mono">{data.ffmpeg.ffmpeg} · {data.ffmpeg.version || "—"}{data.ffmpeg.is_jellyfin ? " · jellyfin-ffmpeg" : ""}</dd>
          <dt>{t("admin.transcode.backend")}</dt><dd className="t-mono">{data.current.backend.toUpperCase()}</dd>
          <dt>{t("admin.transcode.devices")}</dt><dd className="t-mono">{c.devices.join(", ") || t("admin.transcode.noDevices")}</dd>
          <dt>{t("admin.transcode.slots")}</dt><dd className="t-mono">GPU {data.slots.hw_used}/{data.slots.hw_max || "∞"} · CPU {data.slots.sw_used}/{data.slots.sw_max || "∞"}</dd>
          <dt>{t("admin.transcode.lastTest")}</dt><dd>{c.tested_at ? relative(c.tested_at) : "—"}</dd>
        </dl>
      </section>
      {!c.selected && <Alert>{t("admin.transcode.noGpuHelp")}</Alert>}
      <section className="pl-panel">
        <div className="panel-head"><span className="pl-eyebrow">{t("admin.transcode.matrix")}</span></div>
        <CapsMatrix caps={c} />
      </section>
      <section className="pl-panel panel-pad stack">
        <h2 className="t-h3">{t("admin.transcode.dockerTitle")}</h2>
        <p className="t-small muted" style={{ margin: 0 }}>{t("admin.transcode.dockerHelp")}</p>
        <pre className="t-mono" style={{ margin: 0, padding: 12, background: "var(--surface-2)", borderRadius: 10, overflowX: "auto" }}>{`# Intel / AMD
devices:
  - /dev/dri:/dev/dri
# NVIDIA
deploy:
  resources:
    reservations:
      devices:
        - driver: nvidia
          count: all
          capabilities: [gpu]`}</pre>
      </section>
    </div>
  );
}
