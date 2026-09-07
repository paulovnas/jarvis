import { act, renderHook, waitFor } from "@testing-library/react";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { expect, it, vi } from "vitest";
import { useSettingsMenu } from "./use-settings-menu";
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

it("opens settings from the native menu and removes its listener", async () => {
  let callback: EventCallback<unknown> | undefined;
  const cleanup = vi.fn();
  vi.mocked(listen).mockImplementation(async (_event, handler) => { callback = handler; return cleanup; });
  const open = vi.fn();
  const { unmount } = renderHook(() => useSettingsMenu(open));
  await waitFor(() => expect(listen).toHaveBeenCalledWith("app:settings", expect.any(Function)));
  act(() => callback?.({ event: "app:settings", id: 1, payload: null }));
  expect(open).toHaveBeenCalledWith(true);
  unmount();
  expect(cleanup).toHaveBeenCalledOnce();
});
