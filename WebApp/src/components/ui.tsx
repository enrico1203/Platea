import { useEffect, useId, useRef, useState, type InputHTMLAttributes, type ReactNode } from "react";
import { X, Check, AlertTriangle, Info, CircleAlert } from "lucide-react";
import { Mark } from "./Logo";
import { useT, type MessageKey } from "../i18n";
import type { Method } from "../api/types";

export function MethodBadge({ method, reason }: { method: Method; reason?: string | null }) {
  const t = useT();
  const cls = method === "direct_play" ? "pl-badge--direct" : method === "remux" ? "pl-badge--remux" : "pl-badge--transcode";
  const label = t(`method.${method}` as MessageKey);
  return (
    <span className={`pl-badge ${cls}`}>
      <span className="pl-badge__dot" />
      {label}
      {method !== "direct_play" && reason && <span className="pl-badge__code">{reason}</span>}
    </span>
  );
}

export function Badge({ children, tone }: { children: ReactNode; tone?: "danger" | "codec" | "direct" | "transcode" }) {
  const cls = tone === "danger" ? "pl-badge--danger" : tone === "codec" ? "pl-badge--codec" : tone === "direct" ? "pl-badge--direct" : tone === "transcode" ? "pl-badge--transcode" : "";
  return <span className={`pl-badge ${cls}`}>{children}</span>;
}

type FieldProps = InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: ReactNode; error?: string | null; mono?: boolean; icon?: ReactNode };
export function TextField({ label, hint, error, mono, icon, ...rest }: FieldProps) {
  const id = useId();
  return (
    <label className={`pl-field${error ? " is-error" : ""}`} htmlFor={id}>
      <span className="pl-field__label">{label}</span>
      <span className={`pl-field__control${mono ? " is-mono" : ""}`}>
        {icon}
        <input id={id} {...rest} />
      </span>
      {(error || hint) && <span className="pl-field__hint">{error || hint}</span>}
    </label>
  );
}

export function SelectField({ label, hint, value, onChange, options }: { label: string; hint?: ReactNode; value: string; onChange: (v: string) => void; options: { value: string; label: string }[] }) {
  const id = useId();
  return (
    <label className="pl-field" htmlFor={id}>
      <span className="pl-field__label">{label}</span>
      <span className="pl-field__control">
        <select id={id} value={value} onChange={(e) => onChange(e.target.value)}>
          {options.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
      </span>
      {hint && <span className="pl-field__hint">{hint}</span>}
    </label>
  );
}

/** Lo switch applica subito l'impostazione. */
export function Switch({ checked, onChange, label, hint, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: ReactNode; hint?: ReactNode; disabled?: boolean }) {
  const toggle = () => !disabled && onChange(!checked);
  return (
    <span
      className={`pl-switch${checked ? " is-on" : ""}`}
      role="switch"
      aria-checked={checked}
      aria-disabled={disabled}
      tabIndex={disabled ? -1 : 0}
      onClick={toggle}
      onKeyDown={(e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          toggle();
        }
      }}
    >
      <span className="pl-switch__track" />
      <span>
        {label}
        {hint && <span className="pl-switch__hint">{hint}</span>}
      </span>
    </span>
  );
}

export function Dialog({ title, onClose, children, footer }: { title: ReactNode; onClose: () => void; children: ReactNode; footer?: ReactNode }) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  // onClose arriva spesso come funzione nuova a ogni render: in un ref, così l'effetto gira una volta sola
  // e il focus va sulla finestra solo all'apertura (non a ogni tasto premuto in un campo).
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && closeRef.current();
    window.addEventListener("keydown", k);
    ref.current?.focus();
    return () => window.removeEventListener("keydown", k);
  }, []);
  return (
    <div className="dialog-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="dialog" role="dialog" aria-modal="true" tabIndex={-1} ref={ref}>
        <div className="row row--between">
          <h2 className="t-h3">{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}><X /></button>
        </div>
        {children}
        {footer && <div className="row" style={{ justifyContent: "flex-end" }}>{footer}</div>}
      </div>
    </div>
  );
}

export function Confirm({ title, message, confirmLabel, onConfirm, onClose, danger }: { title: string; message: ReactNode; confirmLabel: string; onConfirm: () => void | Promise<void>; onClose: () => void; danger?: boolean }) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  return (
    <Dialog
      title={title}
      onClose={onClose}
      footer={
        <>
          <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>
          <button
            className={`pl-btn ${danger ? "pl-btn--danger" : "pl-btn--primary"}`}
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try { await onConfirm(); onClose(); } finally { setBusy(false); }
            }}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <p className="muted" style={{ margin: 0 }}>{message}</p>
    </Dialog>
  );
}

export function Alert({ tone = "info", children }: { tone?: "info" | "danger" | "warning" | "success"; children: ReactNode }) {
  const Icon = tone === "danger" ? CircleAlert : tone === "warning" ? AlertTriangle : tone === "success" ? Check : Info;
  return (
    <div className={`alert${tone !== "info" ? ` alert--${tone}` : ""}`} role={tone === "danger" ? "alert" : undefined}>
      <Icon />
      <div>{children}</div>
    </div>
  );
}

/** Stato vuoto: marchio monocromatico e una frase che dice cosa fare. */
export function Empty({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      <Mark size={56} mono />
      <div>{children}</div>
      {action}
    </div>
  );
}

let toastTimer: number | undefined;
const toastListeners = new Set<(m: string | null) => void>();
export function toast(message: string) {
  toastListeners.forEach((l) => l(message));
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toastListeners.forEach((l) => l(null)), 3500);
}
export function ToastHost() {
  const [msg, setMsg] = useState<string | null>(null);
  useEffect(() => {
    toastListeners.add(setMsg);
    return () => { toastListeners.delete(setMsg); };
  }, []);
  return msg ? <div className="toast" role="status">{msg}</div> : null;
}
