import { describe, expect, it } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { historyPageSchema, readChat, type ChatSnapshot, type HistoryPage } from "./chat";
import { mergeChat, mergeHistory } from "./chat-history";

function page(start: number, end: number, total = 500): HistoryPage {
  return { conversationId: "c1", history: { start, total }, navigation: [], compactions: [], turns: Array.from({ length: end - start }, (_, i) => ({ ...savedTurn(), id: `turn-${start + i}`, user: `Mensagem ${start + i}` })) };
}
function snapshot(start: number, end: number): ChatSnapshot { return { ...emptyChat(), ...page(start, end) }; }

describe("bounded conversation windows", () => {
  it("loads adjacent pages without duplicates and evicts distant turns", () => {
    let current = snapshot(480, 500);
    for (let start = 460; start >= 0; start -= 20) current = mergeHistory(current, page(start, start + 20), "older");
    expect(current.turns).toHaveLength(60);
    expect(current.history).toEqual({ start: 0, total: 500 });
    expect(current.turns[59].id).toBe("turn-59");
    current = mergeHistory(current, page(60, 80), "newer");
    expect(current.history?.start).toBe(20);
    expect(new Set(current.turns.map(turn => turn.id)).size).toBe(60);
  });
  it("keeps a historical page and live controls when a streaming update arrives", () => {
    const current = snapshot(0, 20);
    const update = { ...snapshot(499, 500), navigation: undefined, revision: 8, activeTurnId: "turn-499", queuedMessages: [{ id: "q", content: "Depois", options: savedTurn().options }] };
    const result = mergeChat(current, update);
    expect(result.turns[0].id).toBe("turn-0");
    expect(result.activeTurnId).toBe("turn-499");
    expect(result.queuedMessages).toHaveLength(1);
    expect(result.navigation?.[0].index).toBe(499);
    expect(result.latestOptions).toEqual(update.turns[0].options);
  });
  it("does not lose the current response when initial history or a page finishes later", () => {
    const live = { ...snapshot(499, 500), revision: 8, activeTurnId: "turn-499" };
    live.turns[0].steps[0] = { ...live.turns[0].steps[0], text: "Resposta mais recente" };
    const loaded = mergeChat(live, { ...snapshot(480, 500), revision: 0 });
    expect(loaded.turns).toHaveLength(20);
    expect(loaded.turns[19].steps[0].text).toBe("Resposta mais recente");
    const history = mergeHistory(loaded, page(490, 500), 499);
    expect(history.turns[9].steps[0].text).toBe("Resposta mais recente");
  });
  it("retains plugin and hook identity across paginated history and a late snapshot", () => {
    const plugin = { component: "plugins" as const, action: "skill_read", status: "applied" as const, summary: "Skill de revisão utilizada", sources: [], durationMs: 1, resourceId: "skill:workspace-tools:review", resourceName: "Review", pluginId: "workspace-tools" };
    const hook = { component: "hooks" as const, action: "PreToolUse", status: "issues" as const, summary: "Hook de comando falhou", sources: [], durationMs: 3, resourceId: "hook:workspace-tools:PreToolUse", resourceName: "Check commands", pluginId: "workspace-tools" };
    const live = { ...snapshot(499, 500), revision: 8, activeTurnId: "turn-499" };
    live.turns[0].steps[0].coreActivities = [hook];
    const older = page(480, 500);
    older.turns[0].steps[0].coreActivities = [plugin];
    const loaded = mergeHistory(readChat(live, "c1"), historyPageSchema.parse(JSON.parse(JSON.stringify(older))), "older");
    expect(loaded.turns.find(turn => turn.id === "turn-480")?.steps[0].coreActivities).toEqual([plugin]);
    expect(loaded.turns.find(turn => turn.id === "turn-499")?.steps[0].coreActivities).toEqual([hook]);

    const stale = readChat({ ...snapshot(490, 500), revision: 0 }, "c1");
    const restored = readChat(JSON.parse(JSON.stringify(mergeChat(loaded, stale))), "c1");
    expect(restored.turns.find(turn => turn.id === "turn-480")?.steps[0].coreActivities).toEqual([plugin]);
    expect(restored.turns.find(turn => turn.id === "turn-499")?.steps[0].coreActivities).toEqual([hook]);
    expect(restored.turns.find(turn => turn.id === "turn-481")?.steps[0].coreActivities).toBeUndefined();
  });
  it("bounds large turns by bytes and restores distant pages and their compactions", () => {
    const large = page(480, 500);
    large.turns = large.turns.map(turn => ({ ...turn, user: "x".repeat(500_000) }));
    const current = mergeHistory(snapshot(460, 480), large, "newer");
    expect(current.turns.length).toBeLessThan(10);
    const older = page(0, 20);
    older.compactions = [{ id: "c", turnId: "turn-1", createdAt: 1, afterTurn: true, automatic: true, tokensBefore: 9000, tokensAfter: 1000 }];
    expect(mergeHistory(current, older, 1).compactions).toEqual(older.compactions);
  });
});
