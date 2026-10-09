import type { CoreSnapshot } from "@/core/core-components";
export function coreFixture(installed = true): CoreSnapshot {
  return { ready: installed, checking: false, items: [
    { id: "context-mode", name: "Context-mode", repository: "https://github.com/mksglu/context-mode" },
    { id: "ponytail", name: "Ponytail", repository: "https://github.com/DietrichGebert/ponytail" },
    { id: "beads", name: "Beads", repository: "https://github.com/gastownhall/beads" },
    { id: "impeccable", name: "Impeccable", repository: "https://github.com/pbakaus/impeccable" },
    { id: "context7", name: "Context7", repository: "https://github.com/upstash/context7" },
    { id: "lsp", name: "Servidores LSP", repository: "https://github.com/typescript-language-server/typescript-language-server" },
    { id: "openmontage", name: "OpenMontage", repository: "https://github.com/calesthio/OpenMontage" },
    { id: "comfyui", name: "ComfyUI", repository: "https://github.com/Comfy-Org/ComfyUI" },
    { id: "graft", name: "Graft", repository: "https://github.com/trailhq/Graft" },
  ].map(item => ({ ...item, id: item.id as CoreSnapshot["items"][number]["id"], installed, configured: installed, installedVersion: installed ? "1.0.0" : null, latestVersion: "1.0.0", updateAvailable: false, stage: null, download: null, error: null, healthError: null, diagnostics: [] })) };
}
