import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CompanionItem } from "@/core/companion";
import { useCompanionNotices, useCompanionNoticeLifetime } from "./use-companion-notices";

function useTimedNotices(paused = false) {
  const notices = useCompanionNotices();
  useCompanionNoticeLifetime(notices.notice?.id ?? null, paused, notices.dismiss);
  return notices;
}

const item = (status: CompanionItem["status"], id: string, updatedAt = 1): CompanionItem => ({
  conversationId: id, agentId: null, projectId: "project", projectName: "Portal", title: id, role: "builder", status,
  global: false, activity: "", durationMs: 1000, activeSince: null, updatedAt, requiresConversation: false, attentionId: `${id}/${status}`, acknowledged: false,
  tasks: [],
});

describe("Jarvito speech queue", () => {
  afterEach(() => vi.useRealTimers());

  it("shows one notice at a time with a fresh 30 seconds for each", async () => {
    vi.useFakeTimers();
    const { result } = renderHook(useTimedNotices);
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
    const { result } = renderHook(useTimedNotices);
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

  it("waits for the whole request to finish instead of announcing completed workers", () => {
    const { result } = renderHook(useCompanionNotices);
    const root = item("running", "request");
    const worker = { ...item("completed", "request"), agentId: "designer", attentionId: "designer/completed" };
    act(() => result.current.sync([root, worker]));
    expect(result.current.notice).toBeNull();
    const question = { ...worker, status: "waiting" as const, attentionId: "designer/question" };
    act(() => result.current.sync([root, question]));
    expect(result.current.notice?.item.agentId).toBe("designer");
    act(() => result.current.sync([root, worker]));
    expect(result.current.notice).toBeNull();
    const finished = { ...root, status: "completed" as const, attentionId: "request/completed" };
    act(() => result.current.sync([finished, worker]));
    expect(result.current.notice?.item.agentId).toBeNull();
    expect(result.current.notice?.item.status).toBe("completed");
  });

  it("recognizes a new question in the same conversation rather than treating it as a duplicate", () => {
    const { result } = renderHook(useCompanionNotices);
    const waiting = { ...item("waiting", "chat"), pendingQuestion: { turnId: "turn", toolId: "one", questions: [{ id: "scope", question: "Qual escopo?", options: [] }] } };
    act(() => result.current.sync([waiting]));
    act(() => result.current.clear());
    act(() => result.current.sync([{ ...waiting, pendingQuestion: { ...waiting.pendingQuestion, toolId: "two" } }]));
    expect(result.current.notice?.item.pendingQuestion?.toolId).toBe("two");
  });

  it("does not consume display time while audio is preparing or playing", async () => {
    vi.useFakeTimers();
    const { result, rerender } = renderHook(({ paused }) => useTimedNotices(paused), { initialProps: { paused: true } });
    act(() => result.current.sync([item("completed", "first"), item("failed", "second", 2)]));
    await act(async () => { await vi.advanceTimersByTimeAsync(90_000); });
    expect(result.current.notice?.item.conversationId).toBe("first");
    rerender({ paused: false });
    await act(async () => { await vi.advanceTimersByTimeAsync(29_999); });
    expect(result.current.notice?.item.conversationId).toBe("first");
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(result.current.notice?.item.conversationId).toBe("second");
  });
});
