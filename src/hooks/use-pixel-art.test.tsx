import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { requestPixelArtSuite, usePixelArt } from "./use-pixel-art";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
let changed: EventCallback<unknown> | undefined;
beforeEach(() => {
  changed = undefined;
  vi.mocked(invoke).mockImplementation(async name => name === "open_pixel_art_suite" ? { conversationId: "chat", sessionId: "chat" } : null);
  vi.mocked(listen).mockImplementation(async (_name, callback) => { changed = callback; return vi.fn<() => void>(); });
});

it("adds an agent-created suite without stealing focus and reopens the same session on request", async () => {
  const { result, unmount } = renderHook(() => usePixelArt("chat"));
  await waitFor(() => expect(changed).toBeDefined());
  act(() => changed?.({ event: "pixel-art-session", id: 1, payload: { conversationId: "other", sessionId: "other" } }));
  expect(result.current.available).toBe(false);
  act(() => changed?.({ event: "pixel-art-session", id: 2, payload: { conversationId: "chat", sessionId: "chat" } }));
  expect(result.current.available).toBe(true);
  expect(result.current.active).toBe(false);
  act(() => requestPixelArtSuite("chat"));
  await waitFor(() => expect(result.current.active).toBe(true));
  expect(invoke).toHaveBeenCalledWith("open_pixel_art_suite", { conversationId: "chat" });
  act(() => result.current.close());
  expect(result.current.active).toBe(false);
  expect(result.current.available).toBe(false);
  expect(vi.mocked(invoke).mock.calls.some(([name]) => name.includes("cancel"))).toBe(false);
  unmount();
  expect(invoke).toHaveBeenCalledWith("set_pixel_art_viewport", { conversationId: "chat", viewport: null });
});

it("shows startup failures without opening an empty suite tab", async () => {
  vi.mocked(invoke).mockImplementation(async name => {
    if (name === "open_pixel_art_suite") throw new Error("O runtime não está disponível.");
    return null;
  });
  const { result } = renderHook(() => usePixelArt("chat"));
  await act(async () => result.current.open());
  expect(result.current.error).toBe("O runtime não está disponível.");
  expect(result.current.active).toBe(false);
  expect(result.current.busy).toBe(false);
});
