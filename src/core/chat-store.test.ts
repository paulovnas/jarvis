import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { emptyChat } from "@/test/chat-fixtures";
import { clearChatStore, getChatSnapshot, subscribeChatSnapshot, updateChatSnapshot } from "./chat-store";

describe("conversation snapshot store", () => {
  beforeEach(clearChatStore);
  afterEach(() => vi.useRealTimers());

  it("shares one revision with every observer", () => {
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = subscribeChatSnapshot("c1", first);
    subscribeChatSnapshot("c1", second);

    updateChatSnapshot("c1", () => ({ ...emptyChat(), revision: 7 }));

    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
    expect(getChatSnapshot("c1")?.revision).toBe(7);
    stopFirst();
  });

  it("keeps the last good snapshot across unmounts and transient failures", () => {
    const stop = subscribeChatSnapshot("c1", vi.fn());
    updateChatSnapshot("c1", () => ({ ...emptyChat(), revision: 9 }));
    stop();

    expect(getChatSnapshot("c1")?.revision).toBe(9);
    updateChatSnapshot("c1", current => current);
    expect(getChatSnapshot("c1")?.revision).toBe(9);
  });

  it("keeps every authoritative revision while publishing a streaming burst once", () => {
    vi.useFakeTimers();
    const revisions: number[] = [];
    subscribeChatSnapshot("c1", () => revisions.push(getChatSnapshot("c1")!.revision));
    for (let revision = 1; revision <= 50; revision++) {
      updateChatSnapshot("c1", () => ({ ...emptyChat(), revision }), "frame");
    }
    expect(getChatSnapshot("c1")?.revision).toBe(50);
    expect(revisions).toEqual([]);
    vi.advanceTimersByTime(60);
    expect(revisions).toEqual([50]);
  });

  it("publishes completion immediately and cancels the pending visual publication", () => {
    vi.useFakeTimers();
    const revisions: number[] = [];
    subscribeChatSnapshot("c1", () => revisions.push(getChatSnapshot("c1")!.revision));
    updateChatSnapshot("c1", () => ({ ...emptyChat(), revision: 1 }), "frame");
    updateChatSnapshot("c1", () => ({ ...emptyChat(), revision: 2 }));
    expect(revisions).toEqual([2]);
    vi.advanceTimersByTime(100);
    expect(revisions).toEqual([2]);
  });

  it("disposes queued publications when the store is cleared or its last observer leaves", () => {
    vi.useFakeTimers();
    const revisions: number[] = [];
    const stop = subscribeChatSnapshot("c1", () => revisions.push(1));
    updateChatSnapshot("c1", () => ({ ...emptyChat(), revision: 1 }), "frame");
    stop();
    subscribeChatSnapshot("c1", () => revisions.push(2));
    vi.advanceTimersByTime(60);
    expect(revisions).toEqual([]);
    updateChatSnapshot("c1", () => ({ ...emptyChat(), revision: 2 }), "frame");
    clearChatStore();
    vi.advanceTimersByTime(60);
    expect(revisions).toEqual([]);
  });
});
