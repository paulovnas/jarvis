import path from "node:path";
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig(({ mode }) => ({
  root: path.resolve(__dirname, "browser-extension"),
  base: "./",
  define: { __JARVIS_FIREFOX__: JSON.stringify(mode === "firefox") },
  plugins: [react(), tailwindcss(), {
    name: "extension-manifest",
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "manifest.json", source: readFileSync(path.resolve(__dirname, mode === "firefox" ? "browser-extension/manifest.firefox.json" : "browser-extension/manifest.json"), "utf8") });
      for (const size of [32, 64, 128]) {
        this.emitFile({ type: "asset", fileName: `icons/${size}.png`, source: readFileSync(path.resolve(__dirname, `src-tauri/icons/${size}x${size}.png`)) });
      }
    },
  }],
  resolve: { alias: { "@": path.resolve(__dirname, "src") } },
  build: {
    outDir: mode === "firefox" ? "dist-firefox" : "dist", emptyOutDir: true,
    rollupOptions: {
      input: { options: path.resolve(__dirname, "browser-extension/options.html"), worker: path.resolve(__dirname, "browser-extension/worker.ts") },
      output: { entryFileNames: "[name].js", chunkFileNames: "chunks/[name]-[hash].js", assetFileNames: "assets/[name]-[hash][extname]" },
    },
  },
}));
