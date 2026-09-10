import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AppUpdate } from "./AppUpdate";
import { APP_VERSION, checkAppUpdate, displayVersion, installAppUpdate, type UpdateInfo, type UpdateProgress } from "@/core/app-update";

vi.mock("@/core/app-update", async original => ({ ...await original<typeof import("@/core/app-update")>(), nativeUpdaterAvailable: () => true, checkAppUpdate: vi.fn(), installAppUpdate: vi.fn() }));
const update: UpdateInfo = { currentVersion: APP_VERSION, installable: true, available: { version: "0.8.0-beta.2", notes: "Melhorias no Jarvis.", publishedAt: null } };
let aboutListener: EventCallback<unknown> | undefined;
const stopListening = vi.fn();
beforeEach(() => {
  vi.mocked(checkAppUpdate).mockReset().mockResolvedValue(update);
  vi.mocked(installAppUpdate).mockReset();
  aboutListener = undefined;
  stopListening.mockReset();
  vi.mocked(listen).mockImplementation(async (name, callback) => {
    if (name === "app:about") aboutListener = callback;
    return stopListening;
  });
});
afterEach(() => vi.useRealTimers());

it("confirma em verde uma verificação manual sem atualização e remove a confirmação ao tentar novamente", async () => {
  vi.mocked(checkAppUpdate).mockResolvedValue({ ...update, available: null });
  const user = userEvent.setup(); render(<AppUpdate />);
  await user.click(screen.getByRole("button", { name: /Sobre o Jarvis/ }));
  expect(screen.queryByText("A versão mais recente já está instalada.")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Verificar atualizações" }));
  const success = await screen.findByRole("status");
  expect(success).toHaveTextContent("A versão mais recente já está instalada.");
  expect(success).toHaveClass("text-onedark-green");
  let failCheck!: (cause: string) => void;
  vi.mocked(checkAppUpdate).mockImplementationOnce(() => new Promise((_, reject) => { failCheck = reject; }));
  await user.click(screen.getByRole("button", { name: "Verificar atualizações" }));
  expect(screen.queryByText("A versão mais recente já está instalada.")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Verificar atualizações" })).toBeDisabled();
  await act(async () => failCheck("GitHub indisponível."));
  expect(screen.getByRole("alert")).toHaveTextContent("GitHub indisponível.");
  expect(screen.queryByText("A versão mais recente já está instalada.")).not.toBeInTheDocument();
});

it("mantém a consulta automática silenciosa quando o aplicativo está atualizado", async () => {
  vi.useFakeTimers();
  vi.mocked(checkAppUpdate).mockResolvedValue({ ...update, available: null });
  const { unmount } = render(<AppUpdate />);
  await act(async () => { vi.advanceTimersByTime(2000); });
  expect(checkAppUpdate).toHaveBeenCalledTimes(1);
  await act(async () => aboutListener?.({ event: "app:about", id: 1, payload: null }));
  expect(screen.getByRole("dialog")).toBeVisible();
  expect(screen.queryByText("A versão mais recente já está instalada.")).not.toBeInTheDocument();
  unmount();
});

it("abre a mesma modal pelo menu nativo e preserva os detalhes da atualização", async () => {
  const user = userEvent.setup();
  const { unmount } = render(<AppUpdate />);
  await waitFor(() => expect(listen).toHaveBeenCalledWith("app:about", expect.any(Function)));
  await act(async () => aboutListener?.({ event: "app:about", id: 1, payload: null }));
  expect(screen.getByText("Paulo Vitor Nascimento")).toBeVisible();
  expect(screen.getByRole("dialog").querySelector("img")).toHaveAttribute("src", "/logo_vertical.png");
  expect(screen.getByRole("heading", { name: "Sobre o Jarvis" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Verificar atualizações" }));
  expect(await screen.findByText("Melhorias no Jarvis.")).toBeVisible();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await act(async () => aboutListener?.({ event: "app:about", id: 2, payload: null }));
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  expect(screen.getByText("Melhorias no Jarvis.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Atualizar e reiniciar" })).toBeEnabled();
  unmount();
  expect(stopListening).toHaveBeenCalledTimes(1);
});

it("apresenta o apoio voluntário por PIX e copia o código completo", async () => {
  const user = userEvent.setup();
  const copy = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  render(<AppUpdate />);
  await user.click(screen.getByRole("button", { name: /Sobre o Jarvis/ }));
  expect(screen.getByRole("heading", { name: "Compre-me um açaí 🫐" })).toBeVisible();
  expect(screen.getByText(/projeto sem fins lucrativos/)).toBeVisible();
  expect(screen.getByRole("img", { name: "QR Code para apoiar o Jarvis via PIX" })).toHaveAttribute("src", "/acai.png");
  await user.click(screen.getByRole("button", { name: "Copiar PIX copia e cola" }));
  expect(copy).toHaveBeenCalledWith("00020101021126540014br.gov.bcb.pix0132nascimento.paulo.vitor@gmail.com5204000053039865802BR5923PAULO V A DE O NASCIMEN6006AMPARO62070503***6304B333");
  expect(screen.getByRole("button", { name: "PIX copiado" })).toBeVisible();
});

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
  expect(screen.getByRole("dialog")).toHaveTextContent(APP_VERSION);
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
