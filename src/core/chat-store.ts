import type { ChatSnapshot } from "./chat";

const MAX_IDLE_CONVERSATIONS = 32;

interface Entry {
  snapshot: ChatSnapshot | null;
  listeners: Set<() => void>;
  touchedAt: number;
  cancelPublication?: () => void;
}

const entries = new Map<string, Entry>();
let clock = 0;

function entry(conversationId: string): Entry {
  const existing = entries.get(conversationId);
  if (existing) {
    existing.touchedAt = ++clock;
    return existing;
  }
  const created: Entry = { snapshot: null, listeners: new Set(), touchedAt: ++clock };
  entries.set(conversationId, created);
  return created;
}

function prune() {
  const idle = [...entries]
    .filter(([, value]) => value.listeners.size === 0)
    .sort(([, left], [, right]) => left.touchedAt - right.touchedAt);
  while (idle.length > MAX_IDLE_CONVERSATIONS) {
    const oldest = idle.shift();
    if (oldest) { oldest[1].cancelPublication?.(); entries.delete(oldest[0]); }
  }
}

function publish(current: Entry, cadence: "immediate" | "frame") {
  if (cadence === "immediate") {
    current.cancelPublication?.();
    current.cancelPublication = undefined;
    current.listeners.forEach(listener => listener());
    return;
  }
  if (current.cancelPublication || current.listeners.size === 0) return;
  let frame: number | undefined;
  const notify = () => publish(current, "immediate");
  // A hidden WebView may suspend animation frames while the stream keeps arriving.
  const timer = setTimeout(notify, 50);
  current.cancelPublication = () => {
    clearTimeout(timer);
    if (frame !== undefined) cancelAnimationFrame(frame);
  };
  if (typeof requestAnimationFrame === "function") frame = requestAnimationFrame(notify);
}

export function getChatSnapshot(conversationId: string | null): ChatSnapshot | null {
  if (!conversationId) return null;
  return entry(conversationId).snapshot;
}

export function updateChatSnapshot(
  conversationId: string,
  update: (current: ChatSnapshot | null) => ChatSnapshot | null,
  cadence: "immediate" | "frame" = "immediate",
): ChatSnapshot | null {
  const current = entry(conversationId);
  const next = update(current.snapshot);
  if (next === current.snapshot) return next;
  current.snapshot = next;
  current.touchedAt = ++clock;
  publish(current, cadence);
  prune();
  return next;
}

export function subscribeChatSnapshot(conversationId: string | null, listener: () => void): () => void {
  if (!conversationId) return () => undefined;
  const current = entry(conversationId);
  current.listeners.add(listener);
  return () => {
    current.listeners.delete(listener);
    if (current.listeners.size === 0) { current.cancelPublication?.(); current.cancelPublication = undefined; }
    current.touchedAt = ++clock;
    prune();
  };
}

export function clearChatStore() {
  entries.forEach(current => current.cancelPublication?.());
  entries.clear();
  clock = 0;
}
