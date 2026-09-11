import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useNotificationFeedback } from "./use-notification-feedback";

const mock = vi.hoisted(() => ({ listen: vi.fn(), invoke: vi.fn(), error: vi.fn(), stop: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mock.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mock.listen }));
vi.mock("sonner", () => ({ toast: { error: mock.error } }));
let emit: (event: { payload: unknown }) => void;
beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
  mock.invoke.mockResolvedValue(snapshot(null).payload);
  mock.listen.mockImplementation(async (name: string, callback: typeof emit) => { if (name === "system:changed") emit = callback; return mock.stop; });
});
const snapshot = (notificationError: string | null, notifications = true) => ({ payload: {
  preferences: { notifications, preventSleep: "off", askUserTimeoutSeconds: 30, responseLanguage: "pt-BR" }, sleepInhibited: false, sleepError: null, notificationError,
} });

it("surfaces failed OS deliveries outside settings without repeating unrelated updates", async () => {
  const { unmount } = renderHook(useNotificationFeedback);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledOnce());
  act(() => {
    emit(snapshot(null));
    emit(snapshot("Permita as notificações nos Ajustes do Sistema."));
    emit(snapshot("Permita as notificações nos Ajustes do Sistema."));
  });
  expect(mock.error).toHaveBeenCalledExactlyOnceWith("Não foi possível enviar a notificação do sistema", expect.objectContaining({ description: "Permita as notificações nos Ajustes do Sistema." }));
  act(() => { emit(snapshot(null)); emit(snapshot("Nova falha")); });
  expect(mock.error).toHaveBeenCalledTimes(2);
  unmount();
  expect(mock.stop).toHaveBeenCalledTimes(2);
  act(() => emit(snapshot("Após fechar")));
  expect(mock.error).toHaveBeenCalledTimes(2);
});

it("does not alert for disabled notifications or malformed events", async () => {
  renderHook(useNotificationFeedback);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledOnce());
  act(() => { emit(snapshot("Falha antiga", false)); emit({ payload: {} }); });
  expect(mock.error).not.toHaveBeenCalled();
});

it("restores an undelivered error on focus and waits until the app is visible to alert", async () => {
  vi.mocked(document.hasFocus).mockReturnValue(false);
  mock.invoke.mockResolvedValue(snapshot("Falha ao entregar no macOS").payload);
  renderHook(useNotificationFeedback);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledOnce());
  expect(mock.error).not.toHaveBeenCalled();
  vi.mocked(document.hasFocus).mockReturnValue(true);
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(mock.error).toHaveBeenCalledExactlyOnceWith("Não foi possível enviar a notificação do sistema", expect.objectContaining({ description: "Falha ao entregar no macOS" }));
});

it("ignores an old initial preference read after a newer delivery error", async () => {
  let resolve!: (value: unknown) => void;
  mock.invoke.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
  renderHook(useNotificationFeedback);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledOnce());
  act(() => emit(snapshot("Erro atual")));
  await act(async () => resolve(snapshot("Erro antigo").payload));
  expect(mock.error).toHaveBeenCalledTimes(1);
});
