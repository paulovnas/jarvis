import { useRef } from "react";
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { companionGeometrySchema, type CompanionGeometry } from "@/core/companion";
import { useIslandMotion } from "./use-island-motion";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => true) }));
const compact: CompanionGeometry = companionGeometrySchema.parse({ expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 288, height: 32, compactX: 0, compactY: 0, compactWidth: 288, compactHeight: 32, surfaceX: 0, surfaceY: 0, surfaceWidth: 288, surfaceHeight: 32 });
const expanded: CompanionGeometry = { ...compact, expanded: true, width: 640, height: 160, compactX: 176, surfaceWidth: 640, surfaceHeight: 160 };
function Surface({ geometry, open, detail = false, visible = true }: { geometry: CompanionGeometry; open: boolean; detail?: boolean; visible?: boolean }) {
  const shell = useRef<HTMLDivElement>(null);
  const pet = useRef<HTMLDivElement>(null);
  useIslandMotion(shell, pet, geometry, open, detail, visible);
  return <div ref={shell} data-testid="island"><div ref={pet} data-testid="pet"><canvas data-testid="canvas" /></div></div>;
}
let callbacks: Map<number, FrameRequestCallback>;
let time = 0;
const frames = (count: number, fps = 30) => {
  act(() => {
    for (let index = 0; index < count; index++) {
      time += 1000 / fps;
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

  it("reveals the island inside a stable viewport without resizing the canvas container", () => {
    const resting = { ...compact, width: 640, height: 600, compactX: 176, surfaceX: 176 };
    const opened = { ...resting, expanded: true, surfaceX: 0, surfaceWidth: 640, surfaceHeight: 160 };
    const view = render(<Surface geometry={resting} open={false} />);
    const shell = view.getByTestId("island");
    const canvas = view.getByTestId("canvas");
    expect(shell.style.width).toBe("640px");
    expect(shell.style.height).toBe("600px");
    expect(shell.style.clipPath).toBe("inset(0px 176px 568px 176px round 18px)");
    view.rerender(<Surface geometry={opened} open />);
    for (let index = 0; index < 90; index++) {
      frames(1);
      expect(shell.style.width).toBe("640px");
      expect(shell.style.height).toBe("600px");
      expect(view.getByTestId("canvas")).toBe(canvas);
    }
    expect(shell.style.clipPath).toBe("inset(0px 0px 440px 0px round 28px)");
    expect(callbacks.size).toBe(0);
  });

  it("keeps the same pet and the compact screen anchor while opening, and settles at 30 fps", () => {
    const view = render(<Surface geometry={compact} open={false} />);
    const pet = view.getByTestId("pet");
    const canvas = view.getByTestId("canvas");
    expect(pet.style.transform).toBe("translate3d(20px, 2px, 0) scale(0.35)");
    expect(pet.style.transformOrigin).toBe("top left");
    view.rerender(<Surface geometry={expanded} open />);
    expect(view.getByTestId("island").style.clipPath).toBe("inset(0px 176px 128px 176px round 18px)");
    expect(pet.style.transform).toBe("translate3d(196px, 2px, 0) scale(0.35)");
    expect(view.getByTestId("pet")).toBe(pet);
    for (let index = 0; index < 90; index++) {
      frames(1);
      expect(pet.style.width).toBe("80px");
      expect(pet.style.height).toBe("80px");
      expect(view.getByTestId("canvas")).toBe(canvas);
    }
    expect(view.getByTestId("island").style.width).toBe("640px");
    expect(view.getByTestId("island").style.height).toBe("160px");
    expect(pet.style.width).toBe("80px");
    expect(pet.style.transform).toBe("translate3d(24px, 55px, 0) scale(1)");
    expect(callbacks.size).toBe(0);
  });

  it.each([60, 120])("paints each display frame at %i fps and stops scheduling once settled", fps => {
    const geometry = { ...expanded, height: 600 };
    const view = render(<Surface geometry={geometry} open={false} />);
    frames(fps * 3, fps);
    view.rerender(<Surface geometry={geometry} open />);
    const shell = view.getByTestId("island");
    for (let index = 0; index < 20; index++) {
      const before = shell.style.clipPath;
      frames(1, fps);
      expect(shell.style.clipPath).not.toBe(before);
      expect(shell.style.width).toBe("640px");
      expect(shell.style.height).toBe("600px");
    }
    frames(fps * 3, fps);
    expect(shell.style.clipPath).toBe("inset(0px 0px 440px 0px round 28px)");
    expect(callbacks.size).toBe(0);
  });

  it("reverses a running transition without teleporting or leaving a blocked transparent frame", () => {
    const view = render(<Surface geometry={expanded} open />);
    frames(4);
    const before = view.getByTestId("island").style.clipPath;
    const petBefore = view.getByTestId("pet").style.transform;
    view.rerender(<Surface geometry={expanded} open={false} />);
    expect(view.getByTestId("island").style.clipPath).toBe(before);
    expect(view.getByTestId("pet").style.transform).toBe(petBefore);
    frames(90);
    expect(view.getByTestId("island").style.clipPath).toBe("inset(0px 176px 128px 176px round 18px)");
    expect(view.getByTestId("pet").style.width).toBe("80px");
    expect(view.getByTestId("pet").style.transform).toBe("translate3d(196px, 2px, 0) scale(0.35)");
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
    expect(view.getByTestId("pet").style.transform).toBe("translate3d(24px, 55px, 0) scale(1)");
    expect(callbacks.size).toBe(0);
    view.rerender(<Surface geometry={expanded} open={false} visible={false} />);
    expect(view.getByTestId("island").style.clipPath).toBe("inset(0px 176px 128px 176px round 18px)");
    expect(view.getByTestId("pet").style.transform).toBe("translate3d(196px, 2px, 0) scale(0.35)");
    expect(callbacks.size).toBe(0);
  });

  it("keeps the Mac pet below the camera while expanded and centered in the physical compact header", () => {
    const geometry: CompanionGeometry = { ...expanded, height: 198, surfaceHeight: 198, compactX: 163, compactWidth: 314, compactHeight: 38, notchWidth: 210, notchHeight: 38, headerHeight: 38, dragAxis: "none" };
    const view = render(<Surface geometry={geometry} open />);
    frames(90);
    expect(view.getByTestId("pet").style.transform).toBe("translate3d(24px, 93px, 0) scale(1)");
    expect(view.getByTestId("island").style.clipPath).toBe("inset(0px 0px 0px 0px round 0 0 28px 28px)");
    view.rerender(<Surface geometry={geometry} open={false} />);
    frames(90);
    expect(view.getByTestId("pet").style.transform).toBe("translate3d(183px, 5px, 0) scale(0.35)");
    expect(view.getByTestId("pet").style.width).toBe("80px");
    expect(view.getByTestId("island").style.clipPath).toBe("inset(0px 163px 160px 163px round 0 0 18px 18px)");
  });

  it("keeps the drawing surface while shrinking the pet for detailed views", () => {
    const view = render(<Surface geometry={expanded} open />);
    frames(90);
    const canvas = view.getByTestId("canvas");
    view.rerender(<Surface geometry={expanded} open detail />);
    frames(90);
    expect(view.getByTestId("canvas")).toBe(canvas);
    expect(view.getByTestId("pet").style.width).toBe("80px");
    expect(view.getByTestId("pet").style.height).toBe("80px");
    expect(view.getByTestId("pet").style.transform).toBe("translate3d(16px, 9px, 0) scale(0.3)");
    expect(invoke).toHaveBeenLastCalledWith("companion_set_hit_rect", { x: 0, y: 0, width: 640, height: 160 });
  });
});
