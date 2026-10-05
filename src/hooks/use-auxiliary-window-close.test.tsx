import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useAuxiliaryWindowClose } from "./use-auxiliary-window-close";

const native = vi.hoisted(() => ({ close: vi.fn().mockResolvedValue(undefined), onCloseRequested: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));
afterEach(() => vi.clearAllMocks());

it("blocks native and explicit closing only while a mutation is in progress", async () => {
  let requested!: (event: { preventDefault: () => void }) => void;
  const dispose = vi.fn();
  native.onCloseRequested.mockImplementation(async callback => { requested = callback; return dispose; });
  const { result, unmount } = renderHook(useAuxiliaryWindowClose);
  await waitFor(() => expect(native.onCloseRequested).toHaveBeenCalled());
  const preventDefault = vi.fn();
  act(() => { result.current.setBusy(true); requested({ preventDefault }); result.current.close(); });
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(native.close).not.toHaveBeenCalled();
  act(() => { result.current.setBusy(false); requested({ preventDefault }); result.current.close(); });
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(native.close).toHaveBeenCalledOnce();
  unmount();
  expect(dispose).toHaveBeenCalledOnce();
});

it("runs cleanup before closing the native window and lets the approved close through", async () => {
  let requested!: (event: { preventDefault: () => void }) => void;
  native.onCloseRequested.mockImplementation(async callback => { requested = callback; return vi.fn(); });
  const { result } = renderHook(useAuxiliaryWindowClose);
  await waitFor(() => expect(native.onCloseRequested).toHaveBeenCalled());
  const cleanup = vi.fn();
  act(() => result.current.setCloseRequest(cleanup));
  const preventDefault = vi.fn();
  act(() => requested({ preventDefault }));
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(cleanup).toHaveBeenCalledOnce();
  expect(native.close).not.toHaveBeenCalled();
  act(() => { result.current.close(); requested({ preventDefault }); });
  expect(native.close).toHaveBeenCalledOnce();
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(cleanup).toHaveBeenCalledOnce();
});
