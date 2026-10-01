import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Button } from "@/components/ui/button";
import { Robot } from "./Robot";

const mock = vi.hoisted(() => {
  const listeners = new Map<string, Set<() => void>>();
  const fields = [
    { name: "status", type: 56, value: 0 },
    { name: "lookX", type: 56, value: 0 },
    { name: "lookY", type: 56, value: 0 },
    ...["hovered", "dragging", "expanded", "walking", "reducedMotion"].map(name => ({ name, type: 59, value: false })),
  ];
  const runtime = {
    isPlaying: false,
    stateMachineInputs: vi.fn(() => fields),
    play: vi.fn(() => { runtime.isPlaying = true; for (const listener of listeners.get("advance") ?? []) listener(); }),
    pause: vi.fn(() => { runtime.isPlaying = false; }),
    startRendering: vi.fn(), stopRendering: vi.fn(), cleanup: vi.fn(),
    on: vi.fn((name: string, callback: () => void) => { if (!listeners.has(name)) listeners.set(name, new Set()); listeners.get(name)?.add(callback); }),
    off: vi.fn((name: string, callback: () => void) => { listeners.get(name)?.delete(callback); }),
  };
  return { runtime, fields, listeners, loaded: true, onLoadError: undefined as (() => void) | undefined, parameters: {} as Record<string, unknown>, setWasmUrl: vi.fn(), setWasmFallbackUrl: vi.fn() };
});

vi.mock("@rive-app/react-canvas", async () => {
  const { useEffect } = await import("react");
  const RiveComponent = () => <canvas data-testid="rive-canvas" aria-hidden="true" />;
  return {
    RuntimeLoader: { setWasmUrl: mock.setWasmUrl, setWasmFallbackUrl: mock.setWasmFallbackUrl },
    Alignment: { Center: "center" }, Fit: { Contain: "contain" }, EventType: { Advance: "advance" },
    StateMachineInputType: { Number: 56, Boolean: 59 }, Layout: class {},
    useRive: (parameters: Record<string, unknown> & { onLoadError?: () => void }) => {
      mock.parameters = parameters; mock.onLoadError = parameters.onLoadError;
      useEffect(() => () => mock.runtime.cleanup(), []);
      return { rive: mock.loaded ? mock.runtime : null, RiveComponent };
    },
  };
});

describe("Jarvito Rive character", () => {
  const motionListeners = new Set<() => void>();
  let motion: MediaQueryList;
  const field = (name: string) => mock.fields.find(input => input.name === name)?.value;

  beforeEach(() => {
    mock.loaded = true; mock.runtime.isPlaying = false; mock.listeners.clear(); motionListeners.clear();
    for (const input of mock.fields) input.value = input.type === 56 ? 0 : false;
    for (const fn of [mock.runtime.play, mock.runtime.pause, mock.runtime.startRendering, mock.runtime.stopRendering, mock.runtime.cleanup, mock.runtime.on, mock.runtime.off]) fn.mockClear();
    mock.runtime.stateMachineInputs.mockReset().mockReturnValue(mock.fields);
    motion = {
      matches: false, media: "(prefers-reduced-motion: reduce)", onchange: null,
      addEventListener: vi.fn((_name: string, listener: () => void) => { motionListeners.add(listener); }),
      removeEventListener: vi.fn((_name: string, listener: () => void) => { motionListeners.delete(listener); }),
      addListener: vi.fn(), removeListener: vi.fn(), dispatchEvent: vi.fn(),
    } as unknown as MediaQueryList;
    vi.spyOn(window, "matchMedia").mockReturnValue(motion);
    vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  });
  afterEach(() => { vi.restoreAllMocks(); });

  it("bundles the character and both WASM binaries locally and preserves the accessible parent button", async () => {
    const clicked = vi.fn();
    const { container } = render(<Button aria-label="Abrir Jarvito" onClick={clicked}><Robot status="idle" /></Button>);
    await screen.findByTestId("rive-canvas");
    expect(container.querySelector("[data-renderer=rive]")).toBeInTheDocument();
    expect(mock.parameters).toMatchObject({ artboard: "Jarvito", stateMachines: "Jarvito", autoplay: false, enableRiveAssetCDN: false, shouldDisableRiveListeners: true });
    for (const url of [mock.parameters.src, mock.setWasmUrl.mock.calls[0]?.[0], mock.setWasmFallbackUrl.mock.calls[0]?.[0]]) {
      expect(url).toEqual(expect.any(String)); expect(url).not.toMatch(/^https?:/);
    }
    expect(screen.getByTestId("rive-canvas")).toHaveAttribute("aria-hidden", "true");
    fireEvent.click(screen.getByRole("button", { name: "Abrir Jarvito" })); expect(clicked).toHaveBeenCalledOnce();
  });

  it("updates the real activity expression without replacing the character canvas", async () => {
    const { rerender, container } = render(<Robot status="running" />);
    const canvas = await screen.findByTestId("rive-canvas");
    expect(field("status")).toBe(1);
    for (const [status, code] of [["waiting", 2], ["reconnecting", 3], ["completed", 4], ["failed", 5], ["idle", 0]] as const) {
      rerender(<Robot status={status} />);
      expect(field("status")).toBe(code); expect(screen.getByTestId("rive-canvas")).toBe(canvas);
      expect(container.querySelector("[data-renderer=rive]")).toHaveAttribute("data-state", status);
    }
    expect(mock.runtime.cleanup).not.toHaveBeenCalled();
  });

  it("reacts to hover and dragging, stops strolling during interaction and constrains gaze", async () => {
    const { rerender } = render(<Robot status="idle" expanded walking lookX={4} lookY={-3} />);
    await screen.findByTestId("rive-canvas");
    expect(field("expanded")).toBe(true); expect(field("walking")).toBe(true);
    expect(field("lookX")).toBe(1); expect(field("lookY")).toBe(-1);
    rerender(<Robot status="idle" walking hovered />);
    expect(field("hovered")).toBe(true); expect(field("walking")).toBe(false);
    rerender(<Robot status="idle" walking dragging lookX={Infinity} />);
    expect(field("dragging")).toBe(true); expect(field("walking")).toBe(false); expect(field("lookX")).toBe(0);
  });

  it("pauses hidden windows and resumes the same instance, with runtime cleanup on unmount", async () => {
    const { rerender, unmount } = render(<Robot status="running" />);
    const canvas = await screen.findByTestId("rive-canvas");
    rerender(<Robot status="running" visible={false} />);
    expect(mock.runtime.pause).toHaveBeenCalled(); expect(mock.runtime.stopRendering).toHaveBeenCalled();
    mock.runtime.play.mockClear();
    rerender(<Robot status="running" visible />);
    expect(mock.runtime.play).toHaveBeenCalledWith("Jarvito"); expect(screen.getByTestId("rive-canvas")).toBe(canvas);
    vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(mock.runtime.isPlaying).toBe(false);
    unmount(); expect(mock.runtime.cleanup).toHaveBeenCalledOnce(); expect(motionListeners.size).toBe(0);
  });

  it("keeps the Rive character visible in reduced motion and applies only one pose per state change", async () => {
    Object.defineProperty(motion, "matches", { configurable: true, value: true });
    const { rerender, container } = render(<Robot status="running" walking />);
    const canvas = await screen.findByTestId("rive-canvas");
    expect(field("reducedMotion")).toBe(true); expect(field("walking")).toBe(false);
    expect(mock.runtime.isPlaying).toBe(false); expect(mock.listeners.get("advance")?.size ?? 0).toBe(0);
    rerender(<Robot status="completed" />);
    expect(field("status")).toBe(4); expect(mock.runtime.isPlaying).toBe(false);
    expect(screen.getByTestId("rive-canvas")).toBe(canvas); expect(container.querySelector("[data-renderer=rive]")).toHaveAttribute("data-motion", "reduced");
    Object.defineProperty(motion, "matches", { configurable: true, value: false });
    act(() => { for (const listener of motionListeners) listener(); });
    expect(mock.runtime.isPlaying).toBe(true); expect(container.querySelector("[data-renderer=rive]")).toHaveAttribute("data-motion", "full");
  });

  it("retains static original artwork during loading or a runtime failure", async () => {
    mock.loaded = false;
    const { rerender, container } = render(<Robot status="waiting" />);
    await screen.findByTestId("rive-canvas");
    expect(container.querySelector("svg[data-renderer=loading]")).toHaveAttribute("data-state", "waiting");
    expect(container.querySelector("svg [class]")).not.toBeInTheDocument();
    act(() => mock.onLoadError?.());
    expect(container.querySelector("svg[data-renderer=fallback]")).toHaveAttribute("data-state", "waiting");
    rerender(<Robot status="failed" />);
    expect(container.querySelector("svg[data-renderer=fallback]")).toHaveAttribute("data-state", "failed");
  });

  it("shows a usable fallback instead of running a malformed character contract", async () => {
    mock.runtime.stateMachineInputs.mockReturnValue(mock.fields.filter(input => input.name !== "status"));
    const { container } = render(<Robot status="failed" />);
    await waitFor(() => expect(container.querySelector("svg[data-renderer=fallback]")).toHaveAttribute("data-state", "failed"));
    expect(mock.runtime.play).not.toHaveBeenCalled(); expect(mock.runtime.stopRendering).toHaveBeenCalled();
  });
});
