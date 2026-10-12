import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useStreamedText } from "./use-streamed-text";

let frames: Map<number, FrameRequestCallback>;
let frameId: number;
let motion: MediaQueryList;
let motionChanges: Set<() => void>;

function frame(elapsed = 16) {
  act(() => {
    vi.advanceTimersByTime(elapsed);
    const pending = [...frames.values()];
    frames.clear();
    pending.forEach(callback => callback(performance.now()));
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  frames = new Map();
  frameId = 0;
  motionChanges = new Set();
  motion = {
    matches: false, media: "(prefers-reduced-motion: reduce)", onchange: null,
    addListener: () => {}, removeListener: () => {}, dispatchEvent: () => true,
    addEventListener: (_type: string, listener: unknown) => { if (typeof listener === "function") motionChanges.add(listener as () => void); },
    removeEventListener: (_type: string, listener: unknown) => { if (typeof listener === "function") motionChanges.delete(listener as () => void); },
  };
  vi.spyOn(window, "matchMedia").mockReturnValue(motion);
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.set(++frameId, callback); return frameId; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(id => { frames.delete(id); });
  Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("streamed presentation text", () => {
  it("coalesces burst arrivals in one frame while the source remains complete", () => {
    const { result, rerender } = renderHook(({ source }) => useStreamedText(source, true, "a"), { initialProps: { source: "Base " } });
    rerender({ source: `Base ${"a".repeat(100)}` });
    rerender({ source: `Base ${"a".repeat(100)}${"b".repeat(100)}` });

    expect(result.current).toBe("Base ");
    expect(frames.size).toBe(1);
    frame();
    expect(result.current.length).toBeGreaterThan(5);
    expect(result.current.length).toBeLessThan(205);
    expect(`Base ${"a".repeat(100)}${"b".repeat(100)}`.startsWith(result.current)).toBe(true);
  });

  it("catches up within a bounded delay even when animation frames stall", () => {
    const { result, rerender } = renderHook(({ source }) => useStreamedText(source, true, "a"), { initialProps: { source: "" } });
    const source = "Long burst ".repeat(1000);
    rerender({ source });
    frame();
    expect(result.current.length).toBeLessThan(source.length);

    act(() => { vi.advanceTimersByTime(160); });
    expect(result.current).toBe(source);
    expect(frames.size).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("increases display progress when arrival rate and backlog grow", () => {
    const slow = renderHook(({ source }) => useStreamedText(source, true, "slow"), { initialProps: { source: "" } });
    const fast = renderHook(({ source }) => useStreamedText(source, true, "fast"), { initialProps: { source: "" } });
    slow.rerender({ source: "a".repeat(100) });
    fast.rerender({ source: "a".repeat(1000) });
    frame();
    expect(fast.result.current.length).toBeGreaterThan(slow.result.current.length);
  });

  it.each(["completed", "cancelled"])("flushes all content immediately when %s", () => {
    const { result, rerender } = renderHook(({ source, streaming }) => useStreamedText(source, streaming, "a"), { initialProps: { source: "", streaming: true } });
    rerender({ source: "Resposta completa", streaming: true });
    expect(result.current).toBe("");
    rerender({ source: "Resposta completa", streaming: false });
    expect(result.current).toBe("Resposta completa");
    expect(frames.size).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("shows replacements, empty resets and stream identity changes without stale text", () => {
    const { result, rerender } = renderHook(({ source, id }) => useStreamedText(source, true, id), { initialProps: { source: "Original", id: "a" } });
    rerender({ source: "Original aguardando", id: "a" });
    expect(result.current).toBe("Original");
    rerender({ source: "Substituição", id: "a" });
    expect(result.current).toBe("Substituição");
    rerender({ source: "", id: "a" });
    expect(result.current).toBe("");
    rerender({ source: "Nova execução", id: "b" });
    expect(result.current).toBe("Nova execução");
    expect(frames.size).toBe(0);
  });

  it("starts a new streaming run from the complete source after an inactive run", () => {
    const { result, rerender } = renderHook(({ source, streaming }) => useStreamedText(source, streaming, "a"), { initialProps: { source: "Primeira", streaming: false } });
    rerender({ source: "Primeira, retomada", streaming: true });
    expect(result.current).toBe("Primeira, retomada");
  });

  it("does not split an emoji surrogate pair while advancing", () => {
    const { result, rerender } = renderHook(({ source }) => useStreamedText(source, true, "a"), { initialProps: { source: "" } });
    rerender({ source: "🙂".repeat(100) });
    frame();
    expect(result.current.length).toBeGreaterThan(0);
    expect(result.current).toMatch(/^(?:🙂)+$/u);
  });

  it("flushes and stops animation when reduced motion becomes preferred", () => {
    const { result, rerender } = renderHook(({ source }) => useStreamedText(source, true, "a"), { initialProps: { source: "" } });
    rerender({ source: "Em andamento" });
    Object.defineProperty(motion, "matches", { configurable: true, value: true });
    act(() => { motionChanges.forEach(listener => listener()); });
    expect(result.current).toBe("Em andamento");
    expect(frames.size).toBe(0);
    rerender({ source: "Em andamento, atualizado" });
    expect(result.current).toBe("Em andamento, atualizado");
  });

  it("flushes on backgrounding and presents hidden-tab arrivals immediately", () => {
    const { result, rerender } = renderHook(({ source }) => useStreamedText(source, true, "a"), { initialProps: { source: "" } });
    rerender({ source: "Em andamento" });
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
    act(() => { document.dispatchEvent(new Event("visibilitychange")); });
    expect(result.current).toBe("Em andamento");
    expect(frames.size).toBe(0);
    rerender({ source: "Em andamento, atualizado" });
    expect(result.current).toBe("Em andamento, atualizado");
  });

  it("cancels the frame, lag deadline and environment listeners on unmount", () => {
    const { rerender, unmount } = renderHook(({ source }) => useStreamedText(source, true, "a"), { initialProps: { source: "" } });
    rerender({ source: "Ainda recebendo" });
    expect(frames.size).toBe(1);
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    unmount();
    expect(frames.size).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
    expect(motionChanges.size).toBe(0);
  });
});
