import type { ChatSnapshot } from "./chat";

const MAX_IDLE_CONVERSATIONS = 32;

interface Entry {
  snapshot: ChatSnapshot | null;
  listeners: Set<() => void>;
  touchedAt: number;
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
    if (oldest) entries.delete(oldest[0]);
  }
}

export function getChatSnapshot(conversationId: string | null): ChatSnapshot | null {
  if (!conversationId) return null;
  return entry(conversationId).snapshot;
}

export function updateChatSnapshot(
  conversationId: string,
  update: (current: ChatSnapshot | null) => ChatSnapshot | null,
): ChatSnapshot | null {
  const current = entry(conversationId);
  const next = update(current.snapshot);
  if (next === current.snapshot) return next;
  current.snapshot = next;
  current.touchedAt = ++clock;
  current.listeners.forEach(listener => listener());
  prune();
  return next;
}

export function subscribeChatSnapshot(conversationId: string | null, listener: () => void): () => void {
  if (!conversationId) return () => undefined;
  const current = entry(conversationId);
  current.listeners.add(listener);
  return () => {
    current.listeners.delete(listener);
    current.touchedAt = ++clock;
    prune();
  };
}

export function clearChatStore() {
  entries.clear();
  clock = 0;
}
