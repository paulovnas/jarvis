import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { StatusBar } from "./StatusBar";

vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => ({ data: null }) }));
vi.mock("../remote/RemoteAccess", () => ({ RemoteAccess: () => <button aria-label="Modo remoto">Remoto</button> }));
vi.mock("./AppUpdate", () => ({ AppUpdate: () => <button aria-label="Versão">Versão</button> }));

afterEach(() => vi.useRealTimers());
it("coloca o acesso remoto entre configurações e versão", async () => {
  render(<StatusBar onOpenSettings={() => {}} />);
  const remote = await screen.findByRole("button", { name: "Modo remoto" });
  const version = await screen.findByRole("button", { name: "Versão" });
  expect(screen.getByRole("button", { name: "Configurações" }).compareDocumentPosition(remote) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(remote.compareDocumentPosition(version) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

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
