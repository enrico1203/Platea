import { Link } from "react-router-dom";

const P = "M14 10h28a10 10 0 0 1 10 10v5.5a10 10 0 0 1-10 10H22V52a2 2 0 0 1-2 2h-6a2 2 0 0 1-2-2V12a2 2 0 0 1 2-2Zm8 6v13.5h20a4 4 0 0 0 4-4V20a4 4 0 0 0-4-4H22Z";

/** Marchio: la P il cui occhiello è uno schermo blu 16:9. Lo schermo è sempre blu. */
export function Mark({ size = 28, mono = false }: { size?: number; mono?: boolean }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
      <path fill="currentColor" fillRule="evenodd" d={P} />
      {!mono && <rect x="22" y="16" width="24" height="13.5" rx="2" fill="var(--blue)" />}
    </svg>
  );
}

/** Lockup: marchio e wordmark minuscolo. */
export function Logo({ to = "/" }: { to?: string }) {
  return (
    <Link to={to} className="logo" aria-label="Platea">
      <Mark />
      <span className="logo__word">platea</span>
    </Link>
  );
}
