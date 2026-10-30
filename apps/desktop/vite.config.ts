import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and no HMR port probing. `--mode web` builds the browser front
// end the server hands out (VITE_DECKLE_TARGET picks transport and platform).
export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  define: mode === "web" ? { "import.meta.env.VITE_DECKLE_TARGET": JSON.stringify("web") } : {},
  // The masthead faces come from the digest crate's fonts (mastheadFonts.ts).
  server: { port: 1420, strictPort: true, fs: { allow: [".", "../../crates/deckle-digest/fonts"] } },
  build: { target: "es2022", outDir: "dist" },
}));
