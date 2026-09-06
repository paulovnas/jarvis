import type { CoreSnapshot } from "@/core/core-components";
export function coreFixture(installed = true): CoreSnapshot {
  return { ready: installed, checking: false, items: [
    { id: "context-mode", name: "Context-mode", repository: "https://github.com/mksglu/context-mode" },
    { id: "ponytail", name: "Ponytail", repository: "https://github.com/DietrichGebert/ponytail" },
    { id: "beads", name: "Beads", repository: "https://github.com/gastownhall/beads" },
    { id: "open-design", name: "Open Design", repository: "https://github.com/nexu-io/open-design" },
  ].map(item => ({ ...item, id: item.id as CoreSnapshot["items"][number]["id"], installed, installedVersion: installed ? "1.0.0" : null, latestVersion: "1.0.0", updateAvailable: false, stage: null, download: null, error: null })) };
}
