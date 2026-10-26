import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and no HMR port probing.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // The masthead faces come from the digest crate's fonts (mastheadFonts.ts).
  server: { port: 1420, strictPort: true, fs: { allow: [".", "../../crates/deckle-digest/fonts"] } },
  build: { target: "es2022", outDir: "dist" },
});
