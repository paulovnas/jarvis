import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { SystemSettings } from "./SystemSettings";
import type { SystemSnapshot } from "@/core/system-preferences";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));
const call = vi.mocked(invoke);
const initial: SystemSnapshot = {
  preferences: { preventSleep: "off", notifications: false, askUserTimeoutSeconds: 30, terminal: { shell: null, arguments: [], fontFamily: null, fontSize: 13 } },
  sleepInhibited: false,
  sleepError: null,
  notificationError: null,
  availableTerminalShells: ["/bin/zsh", "/bin/bash"],
  availableTerminalFonts: ["NotoSansM Nerd Font Mono", "JetBrains Mono"],
  resolvedTerminalShell: "/bin/zsh",
  terminalError: null,
  terminalFontError: null,
};

describe("system preferences", () => {
  beforeEach(() => {
    call.mockReset().mockResolvedValue(initial);
    vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
    vi.mocked(toast.success).mockReset();
  });
  it.each([["open", "Enquanto Jarvis aberto"], ["off", "Desligado"], ["active", "Enquanto houver agentes/chats ativos"]])("saves sleep mode %s without changing notifications", async (value, label) => {
    const user = userEvent.setup();
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, preventSleep: value === "active" ? "off" : "active", notifications: true } });
    render(<SystemSettings />);
    const select = await screen.findByRole("combobox", { name: "Impedir repouso" });
    expect(select).toHaveTextContent(value === "active" ? "Desligado" : "Enquanto houver agentes/chats ativos");
    expect(screen.getByRole("switch", { name: "Notificações do sistema" })).toBeChecked();
      call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, preventSleep: value, notifications: true } });
      await user.click(select); await user.click(await screen.findByRole("option", { name: label }));
      await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { ...initial.preferences, preventSleep: value, notifications: true } }));
      expect(select).toHaveTextContent(label);
  });
  it("enables notifications before testing and can turn them off", async () => {
    const user = userEvent.setup(); render(<SystemSettings />);
    const toggle = await screen.findByRole("switch", { name: "Notificações do sistema" });
    expect(screen.getByRole("button", { name: "Testar" })).toBeDisabled();
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, notifications: true } });
    await user.click(toggle);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { ...initial.preferences, notifications: true } });
    await user.click(screen.getByRole("button", { name: "Testar" }));
    expect(call).toHaveBeenLastCalledWith("test_system_notification");
    expect(toast.success).toHaveBeenCalledWith("Notificação enviada ao sistema", expect.objectContaining({ description: expect.stringContaining("Não incomodar") }));
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
  it("saves the ask_user countdown in seconds and rejects out-of-range values", async () => {
    const user = userEvent.setup(); render(<SystemSettings />);
    const input = await screen.findByRole("spinbutton", { name: "Tempo para resposta recomendada" });
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, askUserTimeoutSeconds: 45 } });
    await user.clear(input); await user.type(input, "45"); await user.tab();
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { ...initial.preferences, askUserTimeoutSeconds: 45 } }));
    expect(input).toHaveValue(45);
    await user.clear(input); await user.type(input, "0"); await user.tab();
    expect(screen.getByRole("alert")).toHaveTextContent("entre 1 e 3.600 segundos");
    expect(input).toHaveValue(45);
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
  it.each(["O macOS recusou a notificação.", "Notificações do Jarvis bloqueadas. Ative Jarvis em Configurações do Windows → Sistema → Notificações."])("reports a native notification delivery failure: %s", async message => {
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, notifications: true } });
    const user = userEvent.setup(); render(<SystemSettings />);
    const test = await screen.findByRole("button", { name: "Testar" });
    call.mockRejectedValue(message);
    await user.click(test);
    expect(await screen.findByRole("alert")).toHaveTextContent(message);
    expect(toast.success).not.toHaveBeenCalled();
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
