import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { beforeEach, expect, it, vi } from "vitest";
import { cancelAppExit, confirmAppExit, getPendingAppExit } from "@/core/app-exit";
import type { AppShutdownStatus } from "@/core/app-update";
import { AppExitDialog } from "./AppExitDialog";

vi.mock("@/core/app-exit", () => ({ getPendingAppExit: vi.fn(), confirmAppExit: vi.fn(), cancelAppExit: vi.fn() }));
vi.mock("@/core/app-update", async original => ({ ...await original<typeof import("@/core/app-update")>(), nativeUpdaterAvailable: () => true }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

const activity: AppShutdownStatus = { activeChats: 0, activeProcesses: 2, restartableProcesses: 1 };
const listeners = new Map<string, EventCallback<unknown>>();
const unlisten = vi.fn();

beforeEach(() => {
  listeners.clear(); unlisten.mockReset(); vi.mocked(toast.error).mockReset();
  vi.mocked(getPendingAppExit).mockReset().mockResolvedValue(null);
  vi.mocked(confirmAppExit).mockReset().mockResolvedValue(undefined);
  vi.mocked(cancelAppExit).mockReset().mockResolvedValue(undefined);
  vi.mocked(listen).mockReset().mockImplementation(async (event, callback) => {
    listeners.set(event, callback as EventCallback<unknown>);
    return unlisten;
  });
});

async function emit(event: string, payload: unknown) {
  await act(async () => { listeners.get(event)?.({ event, id: 1, payload }); });
}

it("recovers a close request made before the global dialog mounted after registering both listeners", async () => {
  vi.mocked(getPendingAppExit).mockImplementation(async () => {
    expect(listeners.has("app:exit-requested")).toBe(true);
    expect(listeners.has("app:exit-error")).toBe(true);
    return activity;
  });
  const { unmount } = render(<AppExitDialog />);
  const dialog = await screen.findByRole("alertdialog", { name: "Fechar o Jarvis?" });
  expect(dialog).toHaveTextContent("Os terminais e processos ativos serão encerrados");
  expect(dialog).toHaveTextContent("As abas dos terminais serão restauradas na próxima abertura");
  expect(dialog).toHaveTextContent("serviços de desenvolvimento elegíveis");
  expect(dialog).toHaveTextContent("Comandos concluídos ou de execução única não serão repetidos");
  expect(confirmAppExit).not.toHaveBeenCalled();
  unmount();
  expect(unlisten).toHaveBeenCalledTimes(2);
});

it("cancels a native close request while preserving the runtime and its processes", async () => {
  const user = userEvent.setup(); render(<AppExitDialog />);
  await waitFor(() => expect(getPendingAppExit).toHaveBeenCalledOnce());
  expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  await emit("app:exit-requested", activity);
  await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Cancelar" }));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  expect(cancelAppExit).toHaveBeenCalledOnce();
  expect(confirmAppExit).not.toHaveBeenCalled();
});

it("warns about active chat execution without suggesting terminal activity when only chats are running", async () => {
  vi.mocked(getPendingAppExit).mockResolvedValue({ activeChats: 1, activeProcesses: 0, restartableProcesses: 0 });
  render(<AppExitDialog />);
  const dialog = await screen.findByRole("alertdialog");
  expect(dialog).toHaveTextContent("As execuções ativas dos chats serão interrompidas");
  expect(dialog).not.toHaveTextContent("terminais");
  expect(dialog).not.toHaveTextContent("serviços de desenvolvimento");
});

it("confirms once, disables concurrent controls and keeps the dialog on a native failure", async () => {
  vi.mocked(getPendingAppExit).mockResolvedValue({ ...activity, activeChats: 1 });
  let fail!: (cause: string) => void;
  vi.mocked(confirmAppExit).mockImplementationOnce(() => new Promise((_, reject) => { fail = reject; }));
  const user = userEvent.setup(); render(<AppExitDialog />);
  const dialog = await screen.findByRole("alertdialog");
  expect(dialog).toHaveTextContent("As execuções ativas dos chats também serão interrompidas");
  await user.dblClick(within(dialog).getByRole("button", { name: "Encerrar e fechar" }));
  expect(within(dialog).getByRole("button", { name: "Fechando…" })).toBeDisabled();
  expect(within(dialog).getByRole("button", { name: "Cancelar" })).toBeDisabled();
  await user.keyboard("{Escape}");
  expect(confirmAppExit).toHaveBeenCalledOnce();
  expect(cancelAppExit).not.toHaveBeenCalled();
  await act(async () => fail("Não foi possível salvar a sessão. Tente novamente."));
  expect(toast.error).toHaveBeenCalledWith("Não foi possível salvar a sessão. Tente novamente.");
  expect(screen.getByRole("alertdialog")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Encerrar e fechar" })).toBeEnabled();
  await user.click(within(dialog).getByRole("button", { name: "Encerrar e fechar" }));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  expect(confirmAppExit).toHaveBeenCalledTimes(2);
});

it("keeps cancellation recoverable if the native runtime does not acknowledge it", async () => {
  vi.mocked(getPendingAppExit).mockResolvedValue(activity);
  vi.mocked(cancelAppExit).mockRejectedValueOnce("Não foi possível cancelar o fechamento.");
  const user = userEvent.setup(); render(<AppExitDialog />);
  await screen.findByRole("alertdialog");
  await user.keyboard("{Escape}");
  await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Não foi possível cancelar o fechamento."));
  expect(screen.getByRole("alertdialog")).toBeVisible();
  expect(screen.getByRole("button", { name: "Cancelar" })).toBeEnabled();
  expect(confirmAppExit).not.toHaveBeenCalled();
});

it("waits for one cancellation acknowledgement without offering a concurrent exit", async () => {
  vi.mocked(getPendingAppExit).mockResolvedValue(activity);
  let finish!: () => void;
  vi.mocked(cancelAppExit).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const user = userEvent.setup(); render(<AppExitDialog />);
  await screen.findByRole("alertdialog");
  await user.dblClick(screen.getByRole("button", { name: "Cancelar" }));
  expect(screen.getByRole("button", { name: "Cancelando…" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Encerrar e fechar" })).toBeDisabled();
  await user.keyboard("{Escape}");
  expect(cancelAppExit).toHaveBeenCalledOnce();
  expect(confirmAppExit).not.toHaveBeenCalled();
  await act(async () => finish());
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
});

it("does not hide a newer native request when the initial pending lookup finishes", async () => {
  let restore!: (status: AppShutdownStatus | null) => void;
  vi.mocked(getPendingAppExit).mockImplementation(() => new Promise(resolve => { restore = resolve; }));
  render(<AppExitDialog />);
  await waitFor(() => expect(getPendingAppExit).toHaveBeenCalledOnce());
  await emit("app:exit-requested", activity);
  await screen.findByRole("alertdialog");
  await act(async () => restore(null));
  expect(screen.getByRole("alertdialog")).toBeVisible();
});

it("shows native exit errors as a toast without confirming or dismissing a pending request", async () => {
  vi.mocked(getPendingAppExit).mockResolvedValue(activity);
  render(<AppExitDialog />);
  await screen.findByRole("alertdialog");
  await emit("app:exit-error", "A restauração dos terminais não pôde ser salva.");
  expect(toast.error).toHaveBeenCalledWith("A restauração dos terminais não pôde ser salva.");
  expect(screen.getByRole("alertdialog")).toBeVisible();
  expect(confirmAppExit).not.toHaveBeenCalled();
  expect(cancelAppExit).not.toHaveBeenCalled();
});

it("rejects malformed close activity without opening an unsafe confirmation", async () => {
  render(<AppExitDialog />);
  await waitFor(() => expect(getPendingAppExit).toHaveBeenCalledOnce());
  await emit("app:exit-requested", { activeProcesses: -1 });
  expect(toast.error).toHaveBeenCalledWith("Não foi possível verificar os processos ativos.");
  expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
});

it("disposes late listener registrations without reading pending state after unmount", async () => {
  const registrations: (() => void)[] = [];
  vi.mocked(listen).mockImplementation(() => new Promise(resolve => { registrations.push(() => resolve(unlisten)); }));
  const { unmount } = render(<AppExitDialog />);
  unmount();
  await act(async () => { registrations.forEach(register => register()); });
  expect(unlisten).toHaveBeenCalledTimes(2);
  expect(getPendingAppExit).not.toHaveBeenCalled();
});
