import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRobotRest } from "./use-robot-rest";

describe("Jarvito rest", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });
  const advance = (ms: number) => act(() => { vi.advanceTimersByTime(ms); });

  it("glances, stretches, then sleeps at exactly two minutes without postponing the deadline", () => {
    const { result, rerender, unmount } = renderHook(() => useRobotRest(false, "idle"));
    advance(25_000); expect(result.current.gesture).toBe("curious");
    advance(2_200); expect(result.current.gesture).toBe("none");
    advance(37_800); expect(result.current.gesture).toBe("stretch");
    advance(2_200); expect(result.current.gesture).toBe("none");
    rerender();
    advance(52_799); expect(result.current.gesture).toBe("none");
    advance(1); expect(result.current.gesture).toBe("sleep");
    advance(120_000); expect(result.current.gesture).toBe("sleep");
    unmount(); expect(vi.getTimerCount()).toBe(0);
  });

  it("wakes for work and starts a fresh idle period when work ends", () => {
    const { result, rerender } = renderHook(({ blocked }) => useRobotRest(blocked, "same-chat"), { initialProps: { blocked: false } });
    advance(119_000);
    rerender({ blocked: true });
    advance(200_000); expect(result.current.gesture).toBe("none");
    rerender({ blocked: false });
    advance(119_999); expect(result.current.gesture).toBe("none");
    advance(1); expect(result.current.gesture).toBe("sleep");
    rerender({ blocked: true }); expect(result.current.gesture).toBe("none");
  });

  it("wakes on deliberate interaction or a new result and grants another full two minutes", () => {
    const { result, rerender } = renderHook(({ key }) => useRobotRest(false, key), { initialProps: { key: "turn-1/completed" } });
    advance(120_000); expect(result.current.gesture).toBe("sleep");
    act(() => result.current.wake()); expect(result.current.gesture).toBe("none");
    advance(119_999); expect(result.current.gesture).toBe("none");
    advance(1); expect(result.current.gesture).toBe("sleep");
    rerender({ key: "turn-2/completed" }); expect(result.current.gesture).toBe("none");
    advance(120_000); expect(result.current.gesture).toBe("sleep");
  });
});
