import { createRef } from "react";
import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useCompanionStroll } from "./use-companion-stroll";

describe("Jarvito island steps", () => {
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

  it("walks in the travel direction, pauses with the native timeline, and rests when hidden", () => {
    vi.useFakeTimers();
    const pet = createRef<HTMLButtonElement>();
    let phase = .25;
    let playState: AnimationPlayState = "running";
    let reduced = false;
    let mediaChanged: (() => void) | undefined;
    vi.spyOn(window, "matchMedia").mockImplementation(() => ({
      get matches() { return reduced; }, media: "", onchange: null,
      addEventListener: (_event: string, changed: EventListenerOrEventListenerObject) => { mediaChanged = changed as () => void; },
      removeEventListener: vi.fn(), addListener: vi.fn(), removeListener: vi.fn(), dispatchEvent: vi.fn(),
    }));
    function Pet({ active }: { active: boolean }) {
      const pose = useCompanionStroll(pet, active);
      return <div ref={element => { if (element) Object.defineProperty(element, "getAnimations", { configurable: true, value: () => [{
        animationName: "companion-stroll", currentTime: phase * 38_300, playState,
        effect: { getTiming: () => ({ duration: 38_300 }) },
      }] }); }}><button ref={pet}>Jarvito</button><output aria-label="Postura">{pose.walking ? "Andando" : "Parado"} · {pose.facing < 0 ? "Ida" : "Volta"}</output></div>;
    }
    const view = render(<Pet active />);
    act(() => vi.advanceTimersByTime(180));
    expect(screen.getByLabelText("Postura")).toHaveTextContent("Andando · Ida");
    playState = "paused";
    act(() => vi.advanceTimersByTime(180));
    expect(screen.getByLabelText("Postura")).toHaveTextContent("Parado · Ida");
    playState = "running"; phase = .65;
    act(() => vi.advanceTimersByTime(180));
    expect(screen.getByLabelText("Postura")).toHaveTextContent("Andando · Volta");
    phase = .8;
    act(() => vi.advanceTimersByTime(180));
    expect(screen.getByLabelText("Postura")).toHaveTextContent("Parado · Volta");
    reduced = true;
    act(() => mediaChanged?.());
    expect(vi.getTimerCount()).toBe(0);
    expect(screen.getByLabelText("Postura")).toHaveTextContent("Parado");
    reduced = false;
    act(() => mediaChanged?.());
    phase = .25;
    act(() => vi.advanceTimersByTime(180));
    expect(screen.getByLabelText("Postura")).toHaveTextContent("Andando");
    view.rerender(<Pet active={false} />);
    expect(screen.getByLabelText("Postura")).toHaveTextContent("Parado");
    expect(vi.getTimerCount()).toBe(0);
    view.unmount();
  });
});
