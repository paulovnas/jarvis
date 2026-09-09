/** Prevent browser reload shortcuts from escaping the desktop app shell. */
export function preventWebviewReload(event: KeyboardEvent) {
  const key = event.key.toLowerCase();
  const reload = event.key === "F5" || ((event.metaKey || event.ctrlKey) && key === "r");
  if (!reload) return;
  event.preventDefault();
  event.stopImmediatePropagation();
}

export function installWebviewShortcutGuards() {
  document.addEventListener("keydown", preventWebviewReload, true);
}
