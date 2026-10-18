import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import capability from "../../../src-tauri/capabilities/default.json";
import { TitleBar } from "./TitleBar";

const native = vi.hoisted(() => ({
  isMaximized: vi.fn(), isFullscreen: vi.fn(), onResized: vi.fn(), startResizeDragging: vi.fn(), close: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(window.navigator, "platform", "get").mockReturnValue("Win32");
  vi.stubGlobal("innerWidth", 1000);
  vi.stubGlobal("innerHeight", 800);
  native.isMaximized.mockResolvedValue(false);
  native.isFullscreen.mockResolvedValue(false);
  native.onResized.mockResolvedValue(() => {});
  native.startResizeDragging.mockResolvedValue(undefined);
  native.close.mockResolvedValue(undefined);
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function pointer(type: string, x: number, y: number, button = 0) {
  const event = new MouseEvent(type, { clientX: x, clientY: y, button, bubbles: true, cancelable: true });
  fireEvent(document.body, event);
  return event;
}

describe("Windows native window resizing", () => {
  it.each([
    ["North", 500, 1, "ns-resize"], ["South", 500, 799, "ns-resize"],
    ["West", 1, 400, "ew-resize"], ["East", 999, 400, "ew-resize"],
    ["NorthWest", 1, 1, "nwse-resize"], ["NorthEast", 999, 1, "nesw-resize"],
    ["SouthWest", 1, 799, "nesw-resize"], ["SouthEast", 999, 799, "nwse-resize"],
  ] as const)("delegates the %s edge to the native window before panel hit testing", async (direction, x, y, cursor) => {
    render(<TitleBar />);
    await waitFor(() => expect(native.onResized).toHaveBeenCalled());
    const panelListener = vi.fn();
    document.addEventListener("pointerdown", panelListener, true);
    try {
      pointer("pointermove", x, y);
      expect(document.documentElement.style.getPropertyValue("--native-window-resize-cursor")).toBe(cursor);
      expect(pointer("pointerdown", x, y).defaultPrevented).toBe(true);
      expect(native.startResizeDragging).toHaveBeenCalledWith(direction);
      expect(panelListener).not.toHaveBeenCalled();
    } finally { document.removeEventListener("pointerdown", panelListener, true); }
  });

  it("leaves interior clicks and non-primary edge buttons alone", async () => {
    render(<TitleBar />);
    await waitFor(() => expect(native.onResized).toHaveBeenCalled());
    expect(pointer("pointerdown", 500, 400).defaultPrevented).toBe(false);
    expect(pointer("pointerdown", 1, 400, 2).defaultPrevented).toBe(false);
    expect(native.startResizeDragging).not.toHaveBeenCalled();
  });

  it("prevents an edge click from also activating a caption control", async () => {
    const view = render(<TitleBar />);
    await waitFor(() => expect(native.onResized).toHaveBeenCalled());
    const close = view.getByRole("button", { name: "Fechar janela" });
    fireEvent.click(close, { clientX: 999, clientY: 1, button: 0, detail: 1 });
    expect(native.close).not.toHaveBeenCalled();
    fireEvent.doubleClick(view.getByRole("banner", { name: "Barra superior" }), { clientX: 500, clientY: 1, button: 0, detail: 2 });
    expect(view.getByRole("button", { name: "Maximizar janela" })).toBeInTheDocument();
    fireEvent.click(close, { clientX: 975, clientY: 20, button: 0, detail: 1 });
    expect(native.close).toHaveBeenCalledOnce();
    fireEvent.click(close, { detail: 0 });
    expect(native.close).toHaveBeenCalledTimes(2);
  });

  it("consumes the click after a rejected edge drag even when released inside a caption button", async () => {
    native.startResizeDragging.mockRejectedValueOnce(new Error("Resize rejected"));
    const view = render(<TitleBar />);
    await waitFor(() => expect(native.onResized).toHaveBeenCalled());
    const close = view.getByRole("button", { name: "Fechar janela" });
    pointer("pointerdown", 999, 1);
    fireEvent.click(close, { clientX: 975, clientY: 20, button: 0, detail: 1 });
    expect(native.close).not.toHaveBeenCalled();
    pointer("pointerdown", 975, 20);
    fireEvent.click(close, { clientX: 975, clientY: 20, button: 0, detail: 1 });
    expect(native.close).toHaveBeenCalledOnce();
    await act(async () => {});
  });

  it("clears the resize cursor after leaving the edge and on unmount", async () => {
    const view = render(<TitleBar />);
    await waitFor(() => expect(native.onResized).toHaveBeenCalled());
    pointer("pointermove", 1, 400);
    expect(document.documentElement).toHaveAttribute("data-native-window-resize");
    pointer("pointermove", 500, 400);
    expect(document.documentElement).not.toHaveAttribute("data-native-window-resize");
    pointer("pointermove", 999, 799);
    view.unmount();
    expect(document.documentElement).not.toHaveAttribute("data-native-window-resize");
    expect(document.documentElement.style.getPropertyValue("--native-window-resize-cursor")).toBe("");
    pointer("pointerdown", 1, 400);
    expect(native.startResizeDragging).not.toHaveBeenCalled();
  });

  it.each(["maximized", "fullscreen"])("does not resize a %s window", async state => {
    native.isMaximized.mockResolvedValue(state === "maximized");
    native.isFullscreen.mockResolvedValue(state === "fullscreen");
    render(<TitleBar />);
    await act(async () => {});
    pointer("pointermove", 1, 400);
    pointer("pointerdown", 1, 400);
    expect(native.startResizeDragging).not.toHaveBeenCalled();
    expect(document.documentElement).not.toHaveAttribute("data-native-window-resize");
  });

  it.each(["MacIntel", "Linux x86_64"])("preserves native border handling on %s", async platform => {
    vi.spyOn(window.navigator, "platform", "get").mockReturnValue(platform);
    render(<TitleBar />);
    await waitFor(() => expect(native.onResized).toHaveBeenCalled());
    pointer("pointerdown", 1, 400);
    expect(native.startResizeDragging).not.toHaveBeenCalled();
  });

  it("allows the main webview to invoke native resize dragging", () => {
    expect(capability.webviews).toEqual(["main"]);
    expect(capability.permissions).toContain("core:window:allow-start-resize-dragging");
  });
});
