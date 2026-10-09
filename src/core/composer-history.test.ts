import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { chatOptions, emptyChat, savedTurn } from "@/test/chat-fixtures";
import type { AgentTurn, HistoryPage } from "./chat";
import { readComposerHistory } from "./composer-history";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

function page(start: number, turns: AgentTurn[]): HistoryPage {
  return { conversationId: "c1", turns, history: { start, total: start + turns.length }, navigation: [], compactions: [] };
}

describe("composer message history", () => {
  beforeEach(() => vi.clearAllMocks());

  it("uses the newest cached window without loading or changing the transcript", async () => {
    const snapshot = { ...emptyChat(), ...page(20, [{ ...savedTurn(), user: "  Texto completo\ncom outra linha.  " }]) };
    const original = structuredClone(snapshot);
    expect(await readComposerHistory("c1", snapshot)).toEqual({ messages: ["  Texto completo\ncom outra linha.  "], before: 20 });
    expect(invokeMock).not.toHaveBeenCalled();
    expect(snapshot).toEqual(original);
  });

  it("accepts a legacy cached snapshot without history metadata", async () => {
    expect(await readComposerHistory("c1", { ...emptyChat(), turns: [savedTurn()] })).toEqual({ messages: ["Leia o README"], before: 0 });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("loads the newest full messages when the transcript is viewing an old window", async () => {
    const snapshot = { ...emptyChat(), turns: [{ ...savedTurn(), user: "Mensagem antiga" }], history: { start: 0, total: 25 } };
    const latest = page(24, [{ ...savedTurn(), user: "Mensagem mais recente" }]);
    latest.navigation = [{ id: "turn1", index: 24, createdAt: 1, user: "Mensagem...", assistant: "" }];
    invokeMock.mockResolvedValueOnce(latest);
    expect(await readComposerHistory("c1", snapshot)).toEqual({ messages: ["Mensagem mais recente"], before: 24 });
    expect(invokeMock).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1" });
    expect(snapshot.turns[0].user).toBe("Mensagem antiga");
  });

  it("loads older full user and auxiliary text in order, omitting empty messages and attachments", async () => {
    const fullText = "Texto completo ".repeat(100);
    invokeMock.mockResolvedValueOnce(page(5, [
      { ...savedTurn(), user: fullText, parts: [{ type: "attachment", attachment: { id: "image", conversationId: "c1", name: "image.png", mime: "image/png", size: 100, kind: "image" } }, { type: "skill", id: "skill", name: "review" }], auxiliaryMessages: [{ id: "a", content: "Complemento\ncompleto", options: chatOptions, parts: [] }, { id: "empty", content: "   ", options: chatOptions, parts: [] }] },
      { ...savedTurn(), id: "turn2", user: "", auxiliaryMessages: [{ id: "b", content: "Último complemento", options: chatOptions, parts: [] }] },
    ]));
    const snapshot = { ...emptyChat(), queuedMessages: [{ id: "q", content: "Ainda na fila", options: chatOptions }] };
    expect(await readComposerHistory("c1", snapshot, 7)).toEqual({ messages: [fullText, "Complemento\ncompleto", "Último complemento"], before: 5 });
    expect(invokeMock).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", before: 7 });
  });

  it("appends the queued messages only to the latest read", async () => {
    const snapshot = { ...emptyChat(), turns: [savedTurn()], queuedMessages: [{ id: "q1", content: "Primeira na fila", options: chatOptions }, { id: "q2", content: "Segunda na fila", options: chatOptions }] };
    expect(await readComposerHistory("c1", snapshot)).toEqual({ messages: ["Leia o README", "Primeira na fila", "Segunda na fila"], before: 0 });
    invokeMock.mockResolvedValueOnce(page(0, [savedTurn()]));
    expect(await readComposerHistory("c1", snapshot, 1)).toEqual({ messages: ["Leia o README"], before: 0 });
  });

  it("loads the latest page without a cached snapshot", async () => {
    invokeMock.mockResolvedValueOnce(page(2, [savedTurn()]));
    expect(await readComposerHistory("c1", undefined)).toEqual({ messages: ["Leia o README"], before: 2 });
    expect(invokeMock).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1" });
  });

  it("rejects history belonging to another conversation", async () => {
    await expect(readComposerHistory("c1", emptyChat("other"))).rejects.toThrow("conversa selecionada");
    expect(invokeMock).not.toHaveBeenCalled();
    invokeMock.mockResolvedValueOnce({ ...page(0, [savedTurn()]), conversationId: "other" });
    await expect(readComposerHistory("c1", undefined)).rejects.toThrow("conversa selecionada");
  });
});
