/** Tema: scuro predefinito; chiaro per chi lo sceglie; "sistema" segue il dispositivo. */
export type Theme = "dark" | "light" | "system";
const KEY = "platea.theme";

export function getTheme(): Theme {
  try { return (localStorage.getItem(KEY) as Theme) || "dark"; } catch { return "dark"; }
}

export function applyTheme(t: Theme = getTheme()) {
  const resolved = t === "system" ? (window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark") : t;
  document.documentElement.dataset.theme = resolved;
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", resolved === "light" ? "#f6f7f9" : "#050608");
}

export function setTheme(t: Theme) {
  try { localStorage.setItem(KEY, t); } catch { /* ignora */ }
  applyTheme(t);
}
