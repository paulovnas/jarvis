import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { SystemSettings } from "./SystemSettings";
import type { SystemSnapshot } from "@/core/system-preferences";
import type { ProviderAccount } from "@/core/provider-accounts";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));
const call = vi.mocked(invoke);
const initial: SystemSnapshot = {
  preferences: { preventSleep: "off", closeBehavior: "tray", notifications: false, companionEnabled: false, messagePredictionsEnabled: true, askUserTimeoutSeconds: 30, responseLanguage: "pt-BR", terminal: { shell: null, arguments: [], fontFamily: null, fontSize: 13 }, browser: { mode: "embedded", application: "chrome" } },
  sleepInhibited: false,
  sleepError: null,
  notificationError: null,
  companionSupported: true,
  traySupported: false,
  companionError: null,
  availableTerminalShells: ["/bin/zsh", "/bin/bash"],
  availableTerminalFonts: ["NotoSansM Nerd Font Mono", "JetBrains Mono"],
  resolvedTerminalShell: "/bin/zsh",
  terminalError: null,
  terminalFontError: null,
};

describe("system preferences", () => {
  it("offers the close behavior when the native tray is supported and saves it independently", async () => {
    call.mockResolvedValue({ ...initial, traySupported: true });
    const user = userEvent.setup();
    const view = render(<SystemSettings />);
    const control = await screen.findByRole("combobox", { name: "Ao fechar o Jarvis" });
    expect(control).toHaveTextContent("Minimizar na bandeja");
    call.mockResolvedValue({ ...initial, traySupported: true, preferences: { ...initial.preferences, closeBehavior: "exit" } });
    await user.click(control);
    await user.click(await screen.findByRole("option", { name: "Encerrar" }));
    await waitFor(() => expect(control).toHaveTextContent("Encerrar"));
    expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { ...initial.preferences, closeBehavior: "exit" } });
    view.unmount();
    call.mockResolvedValue(initial);
    render(<SystemSettings />);
    await screen.findByRole("switch", { name: "Notificações do sistema" });
    expect(screen.queryByRole("combobox", { name: "Ao fechar o Jarvis" })).not.toBeInTheDocument();
  });
  it("groups application, conversations and editor controls by purpose", async () => {
    render(<SystemSettings />);
    const application = await screen.findByRole("region", { name: "Preferências do aplicativo" });
    expect(within(application).getByRole("switch", { name: "Notificações do sistema" })).toBeVisible();
    expect(within(application).getByRole("switch", { name: "Assistente na área de trabalho" })).toBeVisible();
    const conversations = screen.getByRole("region", { name: "Preferências das conversas" });
    expect(within(conversations).getByRole("combobox", { name: "Idioma dos agentes" })).toBeVisible();
    expect(within(conversations).getByRole("spinbutton", { name: "Tempo para resposta recomendada" })).toBeVisible();
    expect(within(conversations).getByRole("button", { name: "Modelo para títulos das conversas" })).toBeVisible();
    const editor = screen.getByRole("region", { name: "Preferências do editor" });
    expect(within(editor).getByRole("button", { name: "Modelo para refinar prompts" })).toBeVisible();
    expect(within(editor).getByRole("switch", { name: "Mostrar previsões" })).toBeVisible();
  });
  it("defaults predictions to enabled and Automatic and hides the model when disabled", async () => {
    const user = userEvent.setup();
    render(<SystemSettings />);
    const toggle = await screen.findByRole("switch", { name: "Mostrar previsões" });
    expect(toggle).toBeChecked();
    expect(screen.getByRole("button", { name: "Modelo para previsão de mensagens" })).toHaveTextContent("Automático");
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, messagePredictionsEnabled: false } });
    await user.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(screen.queryByRole("button", { name: "Modelo para previsão de mensagens" })).not.toBeInTheDocument();
    expect(call).toHaveBeenCalledWith("save_system_preferences", { preferences: { ...initial.preferences, messagePredictionsEnabled: false } });
  });
  beforeEach(() => {
    call.mockReset().mockResolvedValue(initial);
    vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
    vi.mocked(toast.success).mockReset();
  });
  it("saves a prediction model independently and restores Automatic", async () => {
    const account: ProviderAccount = {
      alias: "openai-codex-personal", providerKind: "openai-codex", enabled: true,
      createdAt: 1, email: null, accountType: "unknown", modelsAvailable: true,
      models: [{ id: "predictor", name: "Predictor", reasoningLevels: [], defaultReasoningLevel: null }],
    };
    const user = userEvent.setup();
    render(<SystemSettings accounts={[account]} />);
    const picker = await screen.findByRole("button", { name: "Modelo para previsão de mensagens" });
    const choice = { executor: "jarvis", account: account.alias, model: "predictor", reasoning: null };
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, messagePredictionModel: choice } });
    await user.click(picker);
    await user.hover(await screen.findByRole("menuitem", { name: account.alias }));
    const model = await screen.findByRole("menuitem", { name: "Predictor" });
    act(() => model.focus());
    await user.keyboard("{Enter}");
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { ...initial.preferences, messagePredictionModel: choice } }));
    expect(picker).toHaveTextContent("Predictor");
    expect(screen.getByRole("button", { name: "Modelo para títulos das conversas" })).toHaveTextContent("Automático");
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, messagePredictionModel: null } });
    await user.click(picker);
    await user.click(await screen.findByRole("menuitem", { name: "Automático" }));
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { ...initial.preferences, messagePredictionModel: null } }));
    expect(picker).toHaveTextContent("Automático");
  });
  it("keeps compact controls in the application and conversation groups", async () => {
    render(<SystemSettings />);
    const panel = await screen.findByRole("region", { name: "Preferências do aplicativo" });
    expect(screen.getByRole("combobox", { name: "Idioma dos agentes" })).toHaveAttribute("data-size", "sm");
    expect(within(panel).getByRole("combobox", { name: "Impedir repouso" })).toHaveAttribute("data-size", "sm");
    expect(within(panel).getByRole("switch", { name: "Notificações do sistema" })).toHaveAttribute("data-size", "sm");
    expect(screen.getByRole("spinbutton", { name: "Tempo para resposta recomendada" })).toBeVisible();
  });
  it("enables and disables the optional desktop companion without changing the other preferences", async () => {
    const user = userEvent.setup(); render(<SystemSettings />);
    const toggle = await screen.findByRole("switch", { name: "Assistente na área de trabalho" });
    expect(toggle).not.toBeChecked();
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, companionEnabled: true } });
    await user.click(toggle);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(call).toHaveBeenCalledWith("save_system_preferences", { preferences: { ...initial.preferences, companionEnabled: true } });
    expect(screen.getByText(/arraste a área preta do cabeçalho/)).toBeVisible();
    call.mockResolvedValue(initial);
    await user.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: initial.preferences });
  });
  it("hides the companion toggle on unsupported systems and exposes native window failures", async () => {
    call.mockResolvedValue({ ...initial, companionSupported: false });
    const view = render(<SystemSettings />);
    await screen.findByRole("combobox", { name: "Idioma dos agentes" });
    expect(screen.queryByRole("switch", { name: "Assistente na área de trabalho" })).not.toBeInTheDocument();
    view.unmount();
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, companionEnabled: true }, companionError: "Não foi possível abrir o assistente." });
    render(<SystemSettings />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível abrir o assistente");
    expect(screen.getByRole("switch", { name: "Assistente na área de trabalho" })).toBeChecked();
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
  it("uses pt-BR by default and saves the agents response language", async () => {
    const user = userEvent.setup(); render(<SystemSettings />);
    const select = await screen.findByRole("combobox", { name: "Idioma dos agentes" });
    expect(select).toHaveTextContent("Português (Brasil)");
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, responseLanguage: "en" } });
    await user.click(select); await user.click(await screen.findByRole("option", { name: "English" }));
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { ...initial.preferences, responseLanguage: "en" } }));
    expect(select).toHaveTextContent("English");
    expect(screen.getByText(/A interface permanece em pt-BR\./)).toBeVisible();
  });
  it("saves an independent title model and restores automatic mode without changing other settings", async () => {
    const user = userEvent.setup();
    const account: ProviderAccount = {
      alias: "openai-codex-pessoal", providerKind: "openai-codex", enabled: true,
      createdAt: 1, email: null, accountType: "unknown", modelsAvailable: true,
      models: [{ id: "gpt-6-luna", name: "GPT 6 Luna", reasoningLevels: [], defaultReasoningLevel: null }],
    };
    render(<SystemSettings accounts={[account]} />);
    const picker = await screen.findByRole("button", { name: "Modelo para títulos das conversas" });
    expect(picker).toHaveTextContent("Automático");
    expect(screen.getByText(/com Claude Code, usa um título local/)).toBeVisible();
    const choice = { executor: "jarvis", account: account.alias, model: "gpt-6-luna", reasoning: null };
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, chatTitleModel: choice } });
    await user.click(picker);
    await user.hover(await screen.findByRole("menuitem", { name: account.alias }));
    const model = await screen.findByRole("menuitem", { name: "GPT 6 Luna" });
    act(() => model.focus());
    await user.keyboard("{Enter}");
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", {
      preferences: { ...initial.preferences, chatTitleModel: choice },
    }));
    expect(picker).toHaveTextContent("pessoal · GPT 6 Luna");
    call.mockRejectedValueOnce("Não foi possível salvar o modelo.");
    await user.click(picker);
    await user.click(await screen.findByRole("menuitem", { name: "Automático" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível salvar o modelo");
    expect(picker).toHaveTextContent("GPT 6 Luna");
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, chatTitleModel: null } });
    await user.click(picker);
    await user.click(await screen.findByRole("menuitem", { name: "Automático" }));
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", {
      preferences: { ...initial.preferences, chatTitleModel: null },
    }));
    expect(picker).toHaveTextContent("Automático");
  });
  it("saves a separate refinement model with reasoning and restores or clears it without changing chat titles", async () => {
    const user = userEvent.setup();
    const account: ProviderAccount = {
      alias: "openai-codex-refiner", providerKind: "openai-codex", enabled: true,
      createdAt: 1, email: null, accountType: "unknown", modelsAvailable: true,
      models: [{ id: "refiner", name: "Refiner", reasoningLevels: ["low", "high"], defaultReasoningLevel: "low" }],
    };
    const choice = { executor: "jarvis", account: account.alias, model: "refiner", reasoning: "high" };
    const saved = { ...initial, preferences: { ...initial.preferences, promptRefinementModel: choice } };
    const view = render(<SystemSettings accounts={[account]} />);
    const picker = await screen.findByRole("button", { name: "Modelo para refinar prompts" });
    expect(picker).toHaveTextContent("Não configurado");
    call.mockResolvedValue(saved);
    await user.click(picker);
    await user.hover(await screen.findByRole("menuitem", { name: account.alias }));
    const model = await screen.findByRole("menuitem", { name: "Refiner" });
    act(() => model.focus());
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: saved.preferences }));
    expect(picker).toHaveTextContent("Refiner · Alto");
    expect(screen.getByRole("button", { name: "Modelo para títulos das conversas" })).toHaveTextContent("Automático");
    view.unmount();
    render(<SystemSettings accounts={[account]} />);
    const restored = await screen.findByRole("button", { name: "Modelo para refinar prompts" });
    expect(restored).toHaveTextContent("Refiner · Alto");
    call.mockResolvedValue({ ...initial, preferences: { ...initial.preferences, promptRefinementModel: null } });
    await user.click(restored);
    await user.click(await screen.findByRole("menuitem", { name: "Não configurado" }));
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", { preferences: { ...initial.preferences, promptRefinementModel: null } }));
    expect(restored).toHaveTextContent("Não configurado");
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
  it.each(["O macOS recusou a notificação.", "Notificações do Jarvis bloqueadas. Ative Jarvis em Configurações do Windows → Sistema → Notificações.", "O Linux recusou a notificação. Confira as notificações do Jarvis e o modo Não incomodar no ambiente gráfico."])("reports a native notification delivery failure: %s", async message => {
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
