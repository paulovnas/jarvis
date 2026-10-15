import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { PromptRefinementStatus } from "./PromptRefinementStatus";
import { REFINEMENT_MESSAGES } from "./prompt-refinement-messages";

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });
it("centers the sparkle above the heading and the animated dots below the changing text", () => {
  const view = render(<PromptRefinementStatus />);
  const status = screen.getByRole("status", { name: "Refinando prompt" });
  const heading = screen.getByText("Refinando o texto…");
  const sparkle = view.container.querySelector("svg")!;
  const dots = status.lastElementChild!;
  expect(sparkle.parentElement).toBe(status);
  expect(sparkle.compareDocumentPosition(heading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(dots).toHaveAttribute("aria-hidden", "true");
  expect(dots.previousElementSibling).toHaveTextContent(/.+/);
  expect(dots.parentElement).toBe(status);
  expect(status.lastElementChild).toBe(dots);
  expect(dots.children).toHaveLength(3);
});
it("rotates a different informal status every four seconds and stops on unmount", () => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0);
  const view = render(<PromptRefinementStatus />);
  const status = screen.getByRole("status", { name: "Refinando prompt" });
  expect(REFINEMENT_MESSAGES.length).toBeGreaterThanOrEqual(20);
  expect(status).toHaveTextContent(REFINEMENT_MESSAGES[0]);
  act(() => { vi.advanceTimersByTime(3999); });
  expect(status).toHaveTextContent(REFINEMENT_MESSAGES[0]);
  act(() => { vi.advanceTimersByTime(1); });
  expect(status).toHaveTextContent(REFINEMENT_MESSAGES[1]);
  view.unmount();
  expect(vi.getTimerCount()).toBe(0);
});
