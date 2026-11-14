import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// In sviluppo le chiamate API vanno al server indicato da PLATEA_SERVER_URL.
const server = process.env.PLATEA_SERVER_URL ?? "http://localhost:8096";

export default defineConfig({
  plugins: [react()],
  define: { __APP_VERSION__: JSON.stringify(process.env.npm_package_version ?? "0.1.0") },
  server: {
    port: 5173,
    proxy: { "/api": { target: server, changeOrigin: true, ws: true } },
  },
  build: {
    target: "es2022",
    sourcemap: false,
    chunkSizeWarningLimit: 700,
    rollupOptions: {
      output: {
        manualChunks: { hls: ["hls.js"], react: ["react", "react-dom", "react-router-dom", "@tanstack/react-query"] },
      },
    },
  },
});
