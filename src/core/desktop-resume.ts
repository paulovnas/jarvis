import { listen } from "@tauri-apps/api/event";

/** Native focus also covers WKWebView resumes without a DOM focus event. */
export function onDesktopResume(refresh: () => void): () => void {
  let alive = true;
  let stop: (() => void) | undefined;
  const focus = () => { if (alive) refresh(); };
  const visible = () => { if (document.visibilityState === "visible") focus(); };
  window.addEventListener("focus", focus);
  document.addEventListener("visibilitychange", visible);
  void listen("tauri://focus", event => { if (event.event === "tauri://focus") focus(); })
    .then(unlisten => { if (alive) stop = unlisten; else unlisten(); })
    .catch(() => { /* Browser previews use DOM events. */ });
  return () => {
    alive = false; stop?.();
    window.removeEventListener("focus", focus);
    document.removeEventListener("visibilitychange", visible);
  };
}
