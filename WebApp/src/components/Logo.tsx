import { Link } from "react-router-dom";
import { useState } from "react";
import { serverUrl } from "../api/client";
import { useSession } from "../session";

const P = "M14 10h28a10 10 0 0 1 10 10v5.5a10 10 0 0 1-10 10H22V52a2 2 0 0 1-2 2h-6a2 2 0 0 1-2-2V12a2 2 0 0 1 2-2Zm8 6v13.5h20a4 4 0 0 0 4-4V20a4 4 0 0 0-4-4H22Z";

function PlateaMark({ size, mono }: { size: number; mono: boolean }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
      <path fill="currentColor" fillRule="evenodd" d={P} />
      {!mono && <rect x="22" y="16" width="24" height="13.5" rx="2" fill="var(--blue)" />}
    </svg>
  );
}

/** Iniziale del nome su un quadrato arrotondato: il marchio neutro quando Platea è nascosto e manca un logo. */
function Monogram({ name, size, mono }: { name: string; size: number; mono: boolean }) {
  const initial = [...name].find((c) => /[\p{L}\p{N}]/u.test(c))?.toUpperCase() ?? "•";
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
      <rect width="64" height="64" rx="14" fill={mono ? "currentColor" : "var(--blue)"} opacity={mono ? 0.35 : 1} />
      <text x="32" y="44" textAnchor="middle" fontFamily="var(--font-sans)" fontSize="34" fontWeight="700" fill="#fff">{initial}</text>
    </svg>
  );
}

/**
 * Marchio del server: il logo caricato dall'amministratore; senza logo la P di Platea
 * (la cui asola è uno schermo blu 16:9), oppure l'iniziale del nome se Platea è nascosto.
 */
export function Mark({ size = 28, mono = false }: { size?: number; mono?: boolean }) {
  const info = useSession().brand;
  if (info?.logo) {
    return (
      <img src={`${serverUrl()}${info.logo}`} alt="" aria-hidden="true" className="mark-img"
        style={{ height: size, width: "auto", maxWidth: size * 4, objectFit: "contain", flex: "none", ...(mono ? { filter: "grayscale(1)", opacity: 0.5 } : null) }} />
    );
  }
  if (info?.white_label) return <Monogram name={info.name} size={size} mono={mono} />;
  return <PlateaMark size={size} mono={mono} />;
}

/** Nome scelto dall'amministratore in Impostazioni › Generali; "Platea" se non è stato cambiato. */
export function useServerName(): string {
  const name = useSession().brand?.name.trim();
  return name || "Platea";
}

/** Nome del client da mostrare: "Platea Web" diventa "<nome del server> Web". */
export function useClientLabel(): (client: string) => string {
  const name = useServerName();
  return (client) => client.replace(/^Platea\b/, name);
}

/**
 * Lockup: marchio e nome del server. Col nome predefinito resta il wordmark minuscolo del brand.
 * Un logo largo (un logotipo che contiene già il nome) si mostra da solo.
 */
export function Logo({ to = "/" }: { to?: string }) {
  const name = useServerName();
  const info = useSession().brand;
  const [wide, setWide] = useState(false);
  const isDefault = name.toLowerCase() === "platea" && !info?.white_label;
  return (
    <Link to={to} className="logo" aria-label={name} title={isDefault ? undefined : name}>
      {info?.logo
        ? <img src={`${serverUrl()}${info.logo}`} alt="" className="mark-img" style={{ height: 28, width: "auto", maxWidth: 180, objectFit: "contain", flex: "none" }}
            onLoad={(e) => setWide(e.currentTarget.naturalWidth > e.currentTarget.naturalHeight * 1.8)} />
        : <Mark />}
      {!wide && <span className={`logo__word${isDefault ? "" : " logo__word--custom"}`}>{isDefault ? "platea" : name}</span>}
    </Link>
  );
}
