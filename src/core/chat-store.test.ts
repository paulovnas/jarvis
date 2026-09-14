import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyChat } from "@/test/chat-fixtures";
import { clearChatStore, getChatSnapshot, subscribeChatSnapshot, updateChatSnapshot } from "./chat-store";

describe("conversation snapshot store", () => {
  beforeEach(clearChatStore);

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
});
