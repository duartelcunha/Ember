import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

const r = (p: string) => fileURLToPath(new URL(p, import.meta.url));
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],

  // Nao esconder erros do Rust quando corre via `tauri dev`.
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_ENV_*"],

  resolve: {
    alias: { "@": r("./src") },
  },

  build: {
    // Alvo: o motor WebView2 (Chromium), nao browsers legacy.
    target: "chrome105",
    minify: "esbuild",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      // Multi-page: o bundle do overlay (hot path) nunca carrega codigo de settings.
      input: {
        overlay: r("./overlay.html"),
        settings: r("./settings.html"),
      },
      output: {
        manualChunks: {
          motion: ["motion"],
        },
      },
    },
  },

  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
