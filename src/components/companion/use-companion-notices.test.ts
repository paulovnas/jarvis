import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CompanionItem } from "@/core/companion";
import { useCompanionNotices } from "./use-companion-notices";

const item = (status: CompanionItem["status"], id: string, updatedAt = 1): CompanionItem => ({
  conversationId: id, agentId: null, projectId: "project", projectName: "Portal", title: id, role: "builder", status,
  global: false, activity: "", durationMs: 1000, activeSince: null, updatedAt, requiresConversation: false, attentionId: `${id}/${status}`, acknowledged: false,
  tasks: [],
});

describe("Jarvito speech queue", () => {
  afterEach(() => vi.useRealTimers());

  it("shows one notice at a time with a fresh 30 seconds for each", async () => {
    vi.useFakeTimers();
    const { result } = renderHook(useCompanionNotices);
    act(() => result.current.sync([item("completed", "second", 2), item("waiting", "first", 1), item("failed", "third", 3)]));
    expect(result.current.notice?.item.conversationId).toBe("first");
    await act(async () => { await vi.advanceTimersByTimeAsync(29_999); });
    expect(result.current.notice?.item.conversationId).toBe("first");
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(result.current.notice?.item.conversationId).toBe("second");
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(result.current.notice?.item.conversationId).toBe("third");
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(result.current.notice).toBeNull();
  });

  it("does not reset timers on repeated snapshots or replay a cleared backlog", async () => {
    vi.useFakeTimers();
    const { result } = renderHook(useCompanionNotices);
    const first = item("completed", "first");
    act(() => result.current.sync([first]));
    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
    act(() => result.current.sync([{ ...first, activity: "Updated text" }]));
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(result.current.notice).toBeNull();
    const second = item("failed", "second");
    act(() => result.current.sync([first, second]));
    expect(result.current.notice?.item.conversationId).toBe("second");
    act(() => result.current.clear());
    act(() => result.current.sync([first, second]));
    expect(result.current.notice).toBeNull();
    expect(first.acknowledged).toBe(false);
    expect(second.acknowledged).toBe(false);
  });

  it("removes answered questions and acknowledged results while new attention remains eligible", () => {
    const { result } = renderHook(useCompanionNotices);
    const first = item("waiting", "first");
    const completed = item("completed", "done");
    act(() => result.current.sync([first, completed]));
    expect(result.current.notice?.item.status).toBe("waiting");
    act(() => result.current.sync([{ ...first, status: "running" }, completed]));
    expect(result.current.notice?.item.status).toBe("completed");
    act(() => result.current.sync([{ ...completed, acknowledged: true }]));
    expect(result.current.notice).toBeNull();
    act(() => result.current.sync([{ ...completed, attentionId: "done/next-turn", updatedAt: 5 }]));
    expect(result.current.notice?.id).toBe("done/next-turn");
  });

  it("recognizes a new question in the same conversation rather than treating it as a duplicate", () => {
    const { result } = renderHook(useCompanionNotices);
    const waiting = { ...item("waiting", "chat"), pendingQuestion: { turnId: "turn", toolId: "one", questions: [{ id: "scope", question: "Qual escopo?", options: [] }] } };
    act(() => result.current.sync([waiting]));
    act(() => result.current.clear());
    act(() => result.current.sync([{ ...waiting, pendingQuestion: { ...waiting.pendingQuestion, toolId: "two" } }]));
    expect(result.current.notice?.item.pendingQuestion?.toolId).toBe("two");
  });
});
