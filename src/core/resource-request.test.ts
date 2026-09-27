import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { readResource, ResourceTimeoutError, watchResourceRecovery } from "./resource-request";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.useFakeTimers(); vi.mocked(invoke).mockReset(); });
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

it("releases a stalled read and permits retry without accepting the old response", async () => {
  let finish!: (value: string) => void;
  vi.mocked(invoke).mockReturnValueOnce(new Promise<string>(resolve => { finish = resolve; })).mockResolvedValueOnce("reconnected");
  const expired = readResource("browse_skill_marketplace");
  const rejected = expect(expired).rejects.toBeInstanceOf(ResourceTimeoutError);
  await vi.advanceTimersByTimeAsync(30_001);
  await rejected;
  expect(await readResource("browse_skill_marketplace")).toBe("reconnected");
  finish("old response");
  await expect(expired).rejects.toBeInstanceOf(ResourceTimeoutError);
  expect(vi.getTimerCount()).toBe(0);
});

it("preserves backend errors and clears deadlines after a successful read", async () => {
  const error = { code: "service_unavailable", message: "offline" };
  vi.mocked(invoke).mockRejectedValueOnce(error).mockResolvedValueOnce("local data");
  await expect(readResource("get_core_status")).rejects.toBe(error);
  expect(await readResource("get_core_status")).toBe("local data");
  expect(vi.getTimerCount()).toBe(0);
});

it("recovers on reconnect and periodically without retry storms or listeners after disposal", async () => {
  const online = vi.spyOn(navigator, "onLine", "get").mockReturnValue(false);
  const retry = vi.fn();
  const stop = watchResourceRecovery(retry);
  window.dispatchEvent(new Event("online"));
  expect(retry).not.toHaveBeenCalled();
  online.mockReturnValue(true);
  window.dispatchEvent(new Event("online"));
  window.dispatchEvent(new Event("focus"));
  expect(retry).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(retry).toHaveBeenCalledTimes(2);
  stop();
  await vi.advanceTimersByTimeAsync(60_000);
  window.dispatchEvent(new Event("online"));
  expect(retry).toHaveBeenCalledTimes(2);
  expect(vi.getTimerCount()).toBe(0);
});
