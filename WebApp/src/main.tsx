import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles/tokens.css";
import "./styles/bundle.css";
import "./styles/app.css";
import { applyTheme } from "./lib/theme";
import { initPwa } from "./lib/pwa";
import App from "./App";

applyTheme();
initPwa();
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
