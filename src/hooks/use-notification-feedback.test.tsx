import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useNotificationFeedback } from "./use-notification-feedback";

const mock = vi.hoisted(() => ({ listen: vi.fn(), error: vi.fn(), stop: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mock.listen }));
vi.mock("sonner", () => ({ toast: { error: mock.error } }));
let emit: (event: { payload: unknown }) => void;
beforeEach(() => {
  vi.clearAllMocks();
  mock.listen.mockImplementation(async (_name: string, callback: typeof emit) => { emit = callback; return mock.stop; });
});
const snapshot = (notificationError: string | null, notifications = true) => ({ payload: {
  preferences: { notifications, preventSleep: "off" }, sleepInhibited: false, sleepError: null, notificationError,
} });

it("surfaces failed OS deliveries outside settings without repeating unrelated updates", async () => {
  const { unmount } = renderHook(useNotificationFeedback);
  await waitFor(() => expect(mock.listen).toHaveBeenCalledOnce());
  act(() => {
    emit(snapshot(null));
    emit(snapshot("Permita as notificações nos Ajustes do Sistema."));
    emit(snapshot("Permita as notificações nos Ajustes do Sistema."));
  });
  expect(mock.error).toHaveBeenCalledExactlyOnceWith("Não foi possível enviar a notificação do sistema", expect.objectContaining({ description: "Permita as notificações nos Ajustes do Sistema." }));
  act(() => { emit(snapshot(null)); emit(snapshot("Nova falha")); });
  expect(mock.error).toHaveBeenCalledTimes(2);
  unmount();
  expect(mock.stop).toHaveBeenCalledOnce();
  act(() => emit(snapshot("Após fechar")));
  expect(mock.error).toHaveBeenCalledTimes(2);
});

it("does not alert for disabled notifications or malformed events", async () => {
  renderHook(useNotificationFeedback);
  await waitFor(() => expect(mock.listen).toHaveBeenCalledOnce());
  act(() => { emit(snapshot("Falha antiga", false)); emit({ payload: {} }); });
  expect(mock.error).not.toHaveBeenCalled();
});
