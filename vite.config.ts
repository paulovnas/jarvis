import path from "path";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss(), {
    name: "jarvis-chunk-budgets",
    generateBundle(_options, bundle) {
      for (const asset of Object.values(bundle)) {
        if (asset.type !== "chunk") continue;
        // Monaco is a local, lazy standalone editor. Keep its service graph in one
        // chunk; splitting that graph changes initialization order. All other
        // chunks retain Vite's 500 kB budget, including the initial application.
        const reader = asset.name === "CodeViewer" && Object.keys(asset.modules).some(id => id.includes("monaco-editor"));
        const limit = reader ? 3_000_000 : 500_000;
        if (Buffer.byteLength(asset.code) > limit) this.error(`${asset.fileName} exceeds its ${limit / 1000} kB bundle budget`);
      }
    },
  } satisfies Plugin],
  build: { chunkSizeWarningLimit: 3000 },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // Reference projects under docs contain HTML entry points and unrelated dependencies.
  // The UI registry components import @base-ui/react through deep subpaths, several
  // of which are only reachable from lazily-imported chunks (e.g. the Settings dialog
  // used by onboarding). Vite's entry scan never follows those dynamic imports, so it
  // discovers the deps on first use and forces a full page reload — which wipes React
  // state and resets onboarding to step 1. Pre-bundle every subpath so the optimizer
  // never re-runs mid-session.
  optimizeDeps: {
    entries: ["index.html"],
    include: [
      "@base-ui/react/alert-dialog",
      "@base-ui/react/avatar",
      "@base-ui/react/button",
      "@base-ui/react/checkbox",
      "@base-ui/react/collapsible",
      "@base-ui/react/context-menu",
      "@base-ui/react/dialog",
      "@base-ui/react/input",
      "@base-ui/react/menu",
      "@base-ui/react/merge-props",
      "@base-ui/react/popover",
      "@base-ui/react/progress",
      "@base-ui/react/scroll-area",
      "@base-ui/react/select",
      "@base-ui/react/separator",
      "@base-ui/react/switch",
      "@base-ui/react/tabs",
      "@base-ui/react/toggle",
      "@base-ui/react/toggle-group",
      "@base-ui/react/tooltip",
      "@base-ui/react/use-render",
    ],
  },
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**", "**/docs/**"],
    },
  },
}));
