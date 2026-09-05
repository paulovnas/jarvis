import { describe, expect, it } from "vitest";
import { savedTurn } from "@/test/chat-fixtures";
import { readChat } from "./chat";
import { emptyChat } from "@/test/chat-fixtures";
import { conversationContext } from "./inspector";

describe("conversation context", () => {
  it("uses the latest response usage instead of summing repeated input across steps", () => {
    const turn = savedTurn();
    turn.contextWindow = 1000;
    turn.steps.push({ ...turn.steps[0], tools: [], usage: { inputTokens: 400, outputTokens: 100 } });
    expect(conversationContext([turn])).toMatchObject({ tokens: 500, percent: 50, estimatedTokens: 0 });
  });
  it("estimates subsequent tool results and user messages until another measurement arrives", () => {
    const first = savedTurn();
    first.steps[0].tools[0].output = "12345678";
    const next = savedTurn();
    next.user = "1234";
    next.steps = [];
    next.contextWindow = 2000;
    expect(conversationContext([first, next])).toMatchObject({ tokens: 193, estimatedTokens: 3, limit: 2000 });
    next.steps = [{ ...first.steps[0], tools: [], usage: { inputTokens: 300, outputTokens: 50 } }];
    expect(conversationContext([first, next])).toMatchObject({ tokens: 350, estimatedTokens: 0 });
  });
  it("keeps missing limits and measurements unknown and accepts legacy journals", () => {
    const turn = savedTurn();
    turn.steps[0].tools = [];
    const snapshot = readChat({ ...emptyChat(), turns: [turn] }, "c1");
    expect(conversationContext(snapshot.turns)).toMatchObject({ tokens: 190, limit: null, percent: null });
    turn.steps[0].usage = null;
    expect(conversationContext([turn])).toMatchObject({ tokens: null, percent: null });
    expect(conversationContext([]).tokens).toBeNull();
  });
});
