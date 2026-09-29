import path from "node:path";
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  root: path.resolve(__dirname, "browser-extension"),
  base: "./",
  plugins: [react(), tailwindcss(), {
    name: "extension-manifest",
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "manifest.json", source: readFileSync(path.resolve(__dirname, "browser-extension/manifest.json"), "utf8") });
    },
  }],
  resolve: { alias: { "@": path.resolve(__dirname, "src") } },
  build: {
    outDir: "dist", emptyOutDir: true,
    rollupOptions: {
      input: { options: path.resolve(__dirname, "browser-extension/options.html"), worker: path.resolve(__dirname, "browser-extension/worker.ts") },
      output: { entryFileNames: "[name].js", chunkFileNames: "chunks/[name]-[hash].js", assetFileNames: "assets/[name]-[hash][extname]" },
    },
  },
});
