import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { PixelArtSuite } from "./PixelArtSuite";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));

const pixelFixture = () => ({ conversationId: "chat", available: true, active: true, busy: false, error: null, select: vi.fn(), open: vi.fn(), close: vi.fn() });

beforeEach(() => { vi.mocked(invoke).mockReset().mockResolvedValue(undefined); });
afterEach(async () => { cleanup(); await act(async () => {}); vi.restoreAllMocks(); });

it.each([false, true])("positions a mounted native suite without animation frames when Chromium hidden=%s", async hidden => {
  vi.spyOn(document, "hidden", "get").mockReturnValue(hidden);
  vi.spyOn(window, "requestAnimationFrame").mockReturnValue(1);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 72, 1280, 828));
  render(<PixelArtSuite pixel={pixelFixture()} />);

  await act(async () => {});

  expect(invoke).toHaveBeenCalledWith("set_pixel_art_viewport", { conversationId: "chat", viewport: { x: 0, y: 72, width: 1280, height: 828 } });
  expect(screen.queryByRole("status", { name: "Carregando suíte de pixel art" })).not.toBeInTheDocument();
});

it.each(["dialog", "empty slot"])("keeps the native suite hidden behind a %s without waiting for animation frames", async reason => {
  vi.spyOn(window, "requestAnimationFrame").mockReturnValue(1);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 72, reason === "empty slot" ? 0 : 1280, 828));
  render(<>{reason === "dialog" && <div role="dialog" aria-label="Opções" />}<PixelArtSuite pixel={pixelFixture()} /></>);

  await act(async () => {});

  expect(invoke).toHaveBeenCalledWith("set_pixel_art_viewport", { conversationId: "chat", viewport: null });
  expect(screen.getByRole("status", { name: "Carregando suíte de pixel art" })).toBeVisible();
});

it("deduplicates geometry and serializes a resize behind the pending viewport", async () => {
  let rect = new DOMRect(0, 72, 1280, 828);
  let finish: (() => void) | undefined;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(() => rect);
  vi.mocked(invoke).mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve; }));
  const view = render(<PixelArtSuite pixel={pixelFixture()} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));

  fireEvent.resize(window);
  await act(async () => {});
  expect(invoke).toHaveBeenCalledTimes(1);
  rect = new DOMRect(0, 72, 1000, 700);
  fireEvent.resize(window);
  await act(async () => {});
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(finish).toBeDefined();
  await act(async () => { finish!(); });
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
  expect(invoke).toHaveBeenLastCalledWith("set_pixel_art_viewport", { conversationId: "chat", viewport: { x: 0, y: 72, width: 1000, height: 700 } });
  view.unmount();
  await act(async () => {});
  expect(invoke).toHaveBeenLastCalledWith("set_pixel_art_viewport", { conversationId: "chat", viewport: null });
});

it("ends structural loading after showing the real native viewport and hides it without cancelling on close", async () => {
  const geometry = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 10, y: 50, width: 900, height: 600, top: 50, left: 10, right: 910, bottom: 650, toJSON: () => ({}),
  });
  const pixel = { conversationId: "chat", available: true, active: true, busy: false, error: null, select: vi.fn(), open: vi.fn(), close: vi.fn() };
  try {
    const view = render(<PixelArtSuite pixel={pixel} />);
    expect(screen.getByRole("status", { name: "Carregando suíte de pixel art" })).toBeVisible();
    await waitFor(() => expect(screen.queryByRole("status", { name: "Carregando suíte de pixel art" })).not.toBeInTheDocument());
    expect(invoke).toHaveBeenCalledWith("set_pixel_art_viewport", { conversationId: "chat", viewport: { x: 10, y: 50, width: 900, height: 600 } });
    view.unmount();
    await waitFor(() => expect(invoke).toHaveBeenLastCalledWith("set_pixel_art_viewport", { conversationId: "chat", viewport: null }));
    expect(vi.mocked(invoke).mock.calls.some(([name]) => name === "pixel_art_cancel")).toBe(false);
  } finally { geometry.mockRestore(); }
});
