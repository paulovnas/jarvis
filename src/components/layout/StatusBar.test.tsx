import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { StatusBar } from "./StatusBar";

afterEach(() => vi.useRealTimers());
it("mostra a hora local, atualiza na virada do minuto e limpa o timer", () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(2026, 8, 5, 23, 59, 58));
  const { unmount } = render(<StatusBar />);
  expect(screen.getByLabelText("Hora atual")).toHaveTextContent("23:59");
  act(() => vi.advanceTimersByTime(2000));
  expect(screen.getByLabelText("Hora atual")).toHaveTextContent("00:00");
  act(() => window.dispatchEvent(new Event("focus")));
  expect(vi.getTimerCount()).toBe(1);
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});
