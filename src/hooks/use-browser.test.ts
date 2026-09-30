import { act, renderHook } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import type { BrowserRequest } from "@/core/browser";
import { useBrowser } from "./use-browser";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
beforeEach(() => mocked.mockReset().mockImplementation(async command => command === "get_browser_tabs" ? { tabs: [], activeId: null } : { completed: true }));

it("forwards current IDs, semantic locators, frame pagination and explicit waits to the native contract", async () => {
  const { result } = renderHook(() => useBrowser("chat-1", undefined, false));
  const requests: BrowserRequest[] = [
    { action: "click", id: "native-tab", element: "element-1" },
    { action: "click", id: "ext:epoch:7", locator: { role: "button", name: "Salvar" }, frameId: "frame-1", timeoutMs: 5000 },
    { action: "snapshot", id: "ext:epoch:7", frameId: "frame-1", offset: 10, limit: 100 },
    { action: "wait", id: "ext:epoch:7", locator: { testId: "saved", exact: true }, state: "visible", timeoutMs: 15000 },
    { action: "wait", id: "ext:epoch:7", state: "ready", timeoutMs: 0 },
  ];
  for (const request of requests) {
    await act(async () => { expect(await result.current.command(request)).toEqual({ completed: true }); });
    expect(mocked).toHaveBeenCalledWith("browser_command", { conversationId: "chat-1", request });
  }
});
