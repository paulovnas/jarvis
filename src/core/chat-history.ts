import type { AgentTurn, ChatSnapshot, CompactionEvent, HistoryExcerpt, HistoryPage } from "./chat";

const MAX_TURNS = 60;
const MAX_BYTES = 4 * 1024 * 1024;
const turnSizes = new WeakMap<AgentTurn, number>();
function turnSize(turn: AgentTurn) {
  let size = turnSizes.get(turn);
  if (size === undefined) { size = JSON.stringify(turn).length; turnSizes.set(turn, size); }
  return size;
}
export type HistoryDirection = "older" | "newer" | "latest" | number;
export function historyWindow(snapshot: ChatSnapshot) { return snapshot.history ?? { start: 0, total: snapshot.turns.length }; }

function bound(turns: AgentTurn[], start: number, keep: "start" | "end") {
  let result = turns.slice();
  if (result.length > MAX_TURNS) {
    if (keep === "end") { start += result.length - MAX_TURNS; result = result.slice(-MAX_TURNS); }
    else result = result.slice(0, MAX_TURNS);
  }
  const sizes = result.map(turnSize);
  let bytes = sizes.reduce((sum, size) => sum + size, 0);
  while (result.length > 1 && bytes > MAX_BYTES) {
    if (keep === "end") { bytes -= sizes.shift() ?? 0; result.shift(); start += 1; }
    else { bytes -= sizes.pop() ?? 0; result.pop(); }
  }
  return { turns: result, start };
}
function markers(turns: AgentTurn[], events: CompactionEvent[]) {
  const ids = new Set(turns.map(turn => turn.id));
  return [...new Map(events.filter(event => ids.has(event.turnId)).map(event => [event.id, event])).values()];
}
function navigation(current: HistoryExcerpt[] | undefined, next: ChatSnapshot) {
  if (next.navigation) return next.navigation;
  const result = [...(current ?? [])];
  const last = next.turns[next.turns.length - 1];
  if (last) {
    const item = { id: last.id, index: historyWindow(next).total - 1, createdAt: last.createdAt, user: last.user.slice(0, 160), assistant: (last.steps[last.steps.length - 1]?.text ?? "").slice(0, 160) };
    const index = result.findIndex(entry => entry.id === last.id);
    if (index >= 0) result[index] = item;
    else result.push(item);
  }
  while (result.length > 48) result.splice(1, 1);
  return result;
}

export function mergeChat(current: ChatSnapshot | null, next: ChatSnapshot): ChatSnapshot {
  if (!current || current.conversationId !== next.conversationId) return { ...next, latestOptions: next.turns[next.turns.length - 1]?.options };
  if (!next.history) return current.revision > next.revision ? current : next;
  const newer = next.revision >= current.revision;
  if (!newer && !next.navigation) return current;
  const window = historyWindow(current); const incoming = historyWindow(next);
  let turns = current.turns; let start = window.start;
  const end = start + turns.length;
  if (incoming.start <= end && incoming.start + next.turns.length >= start) {
    const indexed = new Map<number, AgentTurn>();
    const first = newer ? current : next; const last = newer ? next : current;
    first.turns.forEach((turn, index) => indexed.set(historyWindow(first).start + index, turn));
    last.turns.forEach((turn, index) => indexed.set(historyWindow(last).start + index, turn));
    const ordered = [...indexed].sort(([a], [b]) => a - b);
    const bounded = bound(ordered.map(([, turn]) => turn), ordered[0]?.[0] ?? 0, "end");
    turns = bounded.turns; start = bounded.start;
  }
  return { ...(newer ? next : current), turns, history: { start, total: Math.max(window.total, incoming.total) },
    latestOptions: newer ? next.turns[next.turns.length - 1]?.options ?? current.latestOptions : current.latestOptions,
    compactions: markers(turns, [...(current.compactions ?? []), ...(next.compactions ?? [])]),
    navigation: navigation(current.navigation, next),
  };
}

export function mergeHistory(current: ChatSnapshot, page: HistoryPage, direction: HistoryDirection): ChatSnapshot {
  if (current.conversationId !== page.conversationId) return current;
  const window = historyWindow(current);
  let start = page.history.start; let turns = page.turns;
  const end = start + turns.length;
  if ((direction === "older" || direction === "newer") && start <= window.start + current.turns.length && end >= window.start) {
    const indexed = new Map<number, AgentTurn>();
    page.turns.forEach((turn, index) => indexed.set(start + index, turn));
    current.turns.forEach((turn, index) => indexed.set(window.start + index, turn));
    const ordered = [...indexed].sort(([a], [b]) => a - b);
    start = ordered[0]?.[0] ?? 0; turns = ordered.map(([, turn]) => turn);
  } else {
    const live = current.turns.find(turn => turn.id === current.activeTurnId);
    if (live) turns = turns.map(turn => turn.id === live.id ? live : turn);
  }
  const bounded = bound(turns, start, direction === "older" ? "start" : "end");
  return { ...current, turns: bounded.turns, history: { start: bounded.start, total: Math.max(window.total, page.history.total) }, navigation: page.navigation,
    compactions: markers(bounded.turns, [...page.compactions, ...(current.compactions ?? [])]) };
}
