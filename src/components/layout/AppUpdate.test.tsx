import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AppUpdate } from "./AppUpdate";
import { APP_VERSION, checkAppUpdate, displayVersion, installAppUpdate, type UpdateInfo, type UpdateProgress } from "@/core/app-update";

vi.mock("@/core/app-update", async original => ({ ...await original<typeof import("@/core/app-update")>(), nativeUpdaterAvailable: () => true, checkAppUpdate: vi.fn(), installAppUpdate: vi.fn() }));
const update: UpdateInfo = { currentVersion: APP_VERSION, installable: true, available: { version: "0.8.0-beta.2", notes: "Melhorias no Jarvis.", publishedAt: null } };
beforeEach(() => { vi.mocked(checkAppUpdate).mockReset().mockResolvedValue(update); vi.mocked(installAppUpdate).mockReset(); });
afterEach(() => vi.useRealTimers());

it("mostra versão e autoria, consulta ao iniciar e não verifica continuamente ao focar", async () => {
  vi.useFakeTimers();
  const { unmount } = render(<AppUpdate />);
  expect(screen.getByRole("button", { name: `Sobre o Jarvis ${displayVersion(APP_VERSION)}` })).toHaveTextContent(displayVersion(APP_VERSION));
  await act(async () => { vi.advanceTimersByTime(2000); });
  expect(screen.getByRole("button", { name: "Atualização Disponível" })).toBeVisible();
  act(() => { window.dispatchEvent(new Event("focus")); window.dispatchEvent(new Event("focus")); });
  expect(checkAppUpdate).toHaveBeenCalledTimes(1);
  unmount(); expect(vi.getTimerCount()).toBe(0);
});

it("distingue falha de consulta e apresenta os dados do projeto", async () => {
  vi.mocked(checkAppUpdate).mockRejectedValue("GitHub indisponível.");
  const user = userEvent.setup(); render(<AppUpdate />);
  await user.click(screen.getByRole("button", { name: /Sobre o Jarvis/ }));
  expect(screen.getByText("Paulo Vitor Nascimento")).toBeVisible();
  expect(screen.getByText(APP_VERSION)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Verificar atualizações" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("GitHub indisponível.");
  expect(screen.queryByText("Atualização Disponível")).not.toBeInTheDocument();
});

it("apresenta notas, progresso real e mantém a modal aberta até terminar ou falhar", async () => {
  const user = userEvent.setup(); let report!: (event: UpdateProgress) => void; let reject!: (error: string) => void;
  vi.mocked(installAppUpdate).mockImplementation(callback => { report = callback; return new Promise((_, fail) => { reject = fail; }); });
  render(<AppUpdate />);
  await user.click(screen.getByRole("button", { name: /Sobre o Jarvis/ }));
  await user.click(screen.getByRole("button", { name: "Verificar atualizações" }));
  expect(await screen.findByText("Melhorias no Jarvis.")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Atualizar e reiniciar" }));
  act(() => report({ stage: "downloading", downloaded: 50, total: 100 }));
  expect(screen.getByRole("progressbar", { name: "Baixando atualização" })).toHaveAttribute("aria-valuenow", "50");
  await user.keyboard("{Escape}");
  expect(screen.getByRole("dialog")).toBeVisible();
  expect(installAppUpdate).toHaveBeenCalledTimes(1);
  act(() => report({ stage: "verifying" }));
  expect(screen.getByRole("progressbar", { name: "Verificando assinatura" })).not.toHaveAttribute("aria-valuenow");
  await act(async () => reject("Assinatura inválida; nada foi instalado."));
  expect(screen.getByRole("alert")).toHaveTextContent("Assinatura inválida");
  expect(screen.getByRole("button", { name: "Atualizar e reiniciar" })).toBeEnabled();
});

it("permite tentar reabrir após falha do reinício sem pedir outro download", async () => {
  vi.mocked(installAppUpdate).mockImplementation(async callback => { callback({ stage: "restarting" }); throw "A nova janela não confirmou a abertura."; });
  const user = userEvent.setup(); render(<AppUpdate />);
  await user.click(screen.getByRole("button", { name: /Sobre o Jarvis/ }));
  await user.click(screen.getByRole("button", { name: "Verificar atualizações" }));
  await user.click(await screen.findByRole("button", { name: "Atualizar e reiniciar" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Reabrir Jarvis" })).toBeEnabled());
  expect(screen.getByRole("alert")).toHaveTextContent("A nova janela não confirmou");
  const checks = vi.mocked(checkAppUpdate).mock.calls.length;
  const clock = vi.spyOn(Date, "now").mockReturnValue(Date.now() + 6 * 60 * 60_000);
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  clock.mockRestore();
  expect(checkAppUpdate).toHaveBeenCalledTimes(checks);
  expect(screen.getByRole("button", { name: "Reabrir Jarvis" })).toBeEnabled();
});
