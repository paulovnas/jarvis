import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SystemSettings } from "./SystemSettings";
import type { SystemSnapshot } from "@/core/system-preferences";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const call = vi.mocked(invoke);
const initial: SystemSnapshot = { preferences: { preventSleep: "off", notifications: false }, sleepInhibited: false, sleepError: null, notificationError: null };

describe("system preferences", () => {
  beforeEach(() => {
    call.mockReset().mockResolvedValue(initial);
    vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
  });
  it.each([["open", "Enquanto Jarvis aberto"], ["off", "Desligado"], ["active", "Enquanto houver agentes/chats ativos"]])("saves sleep mode %s without changing notifications", async (value, label) => {
    const user = userEvent.setup();
    call.mockResolvedValue({ ...initial, preferences: { preventSleep: value === "active" ? "off" : "active", notifications: true } });
    render(<SystemSettings />);
    const select = await screen.findByRole("combobox", { name: "Impedir repouso" });
    expect(select).toHaveTextContent(value === "active" ? "Desligado" : "Enquanto houver agentes/chats ativos");
    expect(screen.getByRole("switch", { name: "Notificações do sistema" })).toBeChecked();
      call.mockResolvedValue({ ...initial, preferences: { preventSleep: value, notifications: true } });
      await user.click(select); await user.click(await screen.findByRole("option", { name: label }));
      await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { preventSleep: value, notifications: true } }));
      expect(select).toHaveTextContent(label);
  });
  it("enables notifications before testing and can turn them off", async () => {
    const user = userEvent.setup(); render(<SystemSettings />);
    const toggle = await screen.findByRole("switch", { name: "Notificações do sistema" });
    expect(screen.getByRole("button", { name: "Testar" })).toBeDisabled();
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, notifications: true } });
    await user.click(toggle);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { preventSleep: "off", notifications: true } });
    await user.click(screen.getByRole("button", { name: "Testar" }));
    expect(call).toHaveBeenLastCalledWith("test_system_notification");
    call.mockResolvedValue(initial); await user.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(screen.getByRole("button", { name: "Testar" })).toBeDisabled();
  });
  it("preserves the previous preference when permission is denied", async () => {
    const user = userEvent.setup(); render(<SystemSettings />);
    const toggle = await screen.findByRole("switch", { name: "Notificações do sistema" });
    call.mockRejectedValue("Notificações bloqueadas. Ative Jarvis em Ajustes do Sistema → Notificações.");
    await user.click(toggle);
    expect(await screen.findByRole("alert")).toHaveTextContent("Notificações bloqueadas");
    expect(toggle).not.toBeChecked(); expect(toggle).toBeEnabled();
  });
  it("uses skeletons while loading and allows retrying a failed load", async () => {
    let reject: (error: Error) => void = () => {};
    call.mockReturnValueOnce(new Promise((_resolve, fail) => { reject = fail; }));
    const user = userEvent.setup(); render(<SystemSettings />);
    expect(screen.getByRole("status", { name: "Carregando preferências do sistema" })).toBeVisible();
    await waitFor(() => expect(call).toHaveBeenCalled());
    reject(new Error("Falha de leitura"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Falha de leitura");
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(await screen.findByRole("combobox", { name: "Impedir repouso" })).toHaveTextContent("Desligado");
  });
  it("reports a native notification delivery failure", async () => {
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, notifications: true } });
    const user = userEvent.setup(); render(<SystemSettings />);
    const test = await screen.findByRole("button", { name: "Testar" });
    call.mockRejectedValue("O macOS recusou a notificação.");
    await user.click(test);
    expect(await screen.findByRole("alert")).toHaveTextContent("O macOS recusou");
    expect(test).toBeEnabled();
  });
  it("updates native service failures live and removes its subscription on close", async () => {
    const stop = vi.fn();
    let changed: (payload: unknown) => void = () => {};
    vi.mocked(listen).mockImplementationOnce(async (_event, callback) => {
      changed = payload => callback({ event: "system:changed", id: 1, payload });
      return stop;
    });
    const view = render(<SystemSettings />);
    await screen.findByRole("combobox", { name: "Impedir repouso" });
    act(() => changed({ ...initial, sleepError: "Não foi possível impedir o repouso neste sistema." }));
    expect(screen.getByRole("alert")).toHaveTextContent("Não foi possível impedir o repouso");
    view.unmount(); expect(stop).toHaveBeenCalledOnce();
  });
});
