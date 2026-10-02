import { useRef } from "react";
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { companionGeometrySchema, type CompanionGeometry } from "@/core/companion";
import { useIslandMotion } from "./use-island-motion";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => true) }));
const compact: CompanionGeometry = companionGeometrySchema.parse({ expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 288, height: 32, compactX: 0, compactY: 0, compactWidth: 288, compactHeight: 32, surfaceX: 0, surfaceY: 0, surfaceWidth: 288, surfaceHeight: 32 });
const expanded: CompanionGeometry = { ...compact, expanded: true, width: 640, height: 160, compactX: 176, surfaceWidth: 640, surfaceHeight: 160 };
function Surface({ geometry, open, visible = true }: { geometry: CompanionGeometry; open: boolean; visible?: boolean }) {
  const shell = useRef<HTMLDivElement>(null);
  const pet = useRef<HTMLDivElement>(null);
  useIslandMotion(shell, pet, geometry, open, false, visible);
  return <div ref={shell} data-testid="island"><div ref={pet} data-testid="pet" /></div>;
}
let callbacks: Map<number, FrameRequestCallback>;
let time = 0;
const frames = (count: number) => {
  act(() => {
    for (let index = 0; index < count; index++) {
      time += 1000 / 30;
      const pending = [...callbacks.values()]; callbacks.clear();
      for (const callback of pending) callback(time);
    }
  });
};
describe("Island motion", () => {
  beforeEach(() => {
    callbacks = new Map(); time = 0;
    let id = 0;
    vi.spyOn(performance, "now").mockImplementation(() => time);
    vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { callbacks.set(++id, callback); return id; });
    vi.spyOn(window, "cancelAnimationFrame").mockImplementation(frame => { callbacks.delete(frame); });
    vi.mocked(invoke).mockClear();
  });
  afterEach(() => vi.restoreAllMocks());

  it("keeps the same pet and the compact screen anchor while opening, and settles at 30 fps", () => {
    const view = render(<Surface geometry={compact} open={false} />);
    const pet = view.getByTestId("pet");
    view.rerender(<Surface geometry={expanded} open />);
    expect(view.getByTestId("island").style.left).toBe("176px");
    expect(view.getByTestId("island").style.width).toBe("288px");
    expect(view.getByTestId("pet")).toBe(pet);
    frames(90);
    expect(view.getByTestId("island").style.width).toBe("640px");
    expect(view.getByTestId("island").style.height).toBe("160px");
    expect(pet.style.width).toBe("80px");
    expect(callbacks.size).toBe(0);
  });

  it("reverses a running transition without teleporting or leaving a blocked transparent frame", () => {
    const view = render(<Surface geometry={expanded} open />);
    frames(4);
    const before = view.getByTestId("island").style.width;
    view.rerender(<Surface geometry={expanded} open={false} />);
    expect(view.getByTestId("island").style.width).toBe(before);
    frames(90);
    expect(view.getByTestId("island").style.width).toBe("288px");
    expect(view.getByTestId("island").style.left).toBe("176px");
    expect(view.getByTestId("pet").style.width).toBe("28px");
    expect(view.getByTestId("pet").style.top).toBe("2px");
    expect(invoke).toHaveBeenLastCalledWith("companion_set_hit_rect", { x: 176, y: 0, width: 288, height: 32 });
    for (const [, arguments_] of vi.mocked(invoke).mock.calls) {
      const bounds = arguments_ as { x: number; y: number; width: number; height: number };
      expect(bounds.width + bounds.x).toBeLessThanOrEqual(expanded.width);
      expect(bounds.height + bounds.y).toBeLessThanOrEqual(expanded.height);
    }
  });

  it("uses the final layout immediately when motion is reduced or the window is hidden", () => {
    vi.spyOn(window, "matchMedia").mockReturnValue({ matches: true } as MediaQueryList);
    const view = render(<Surface geometry={expanded} open />);
    expect(view.getByTestId("island").style.width).toBe("640px");
    expect(callbacks.size).toBe(0);
    view.rerender(<Surface geometry={expanded} open={false} visible={false} />);
    expect(view.getByTestId("island").style.width).toBe("288px");
    expect(callbacks.size).toBe(0);
  });

  it("keeps the Mac pet below the camera while expanded and centered in the physical compact header", () => {
    const geometry: CompanionGeometry = { ...expanded, height: 198, surfaceHeight: 198, compactX: 163, compactWidth: 314, compactHeight: 38, notchWidth: 210, notchHeight: 38, headerHeight: 38, dragAxis: "none" };
    const view = render(<Surface geometry={geometry} open />);
    frames(90);
    expect(view.getByTestId("pet").style.top).toBe("93px");
    expect(view.getByTestId("island").style.borderRadius).toBe("0 0 28px 28px");
    view.rerender(<Surface geometry={geometry} open={false} />);
    frames(90);
    expect(view.getByTestId("pet").style.top).toBe("5px");
    expect(view.getByTestId("pet").style.width).toBe("28px");
    expect(view.getByTestId("island").style.height).toBe("38px");
    expect(view.getByTestId("island").style.width).toBe("314px");
  });
});
