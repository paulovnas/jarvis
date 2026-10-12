import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { writeClipboardText } from "@/core/clipboard";
import { systemSnapshotSchema } from "@/core/system-preferences";
import { BrowserSettings } from "./BrowserSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/core/clipboard", () => ({ writeClipboardText: vi.fn() }));
const call = vi.mocked(invoke);
const initial = systemSnapshotSchema.parse({ preferences: { preventSleep: "active", notifications: true, askUserTimeoutSeconds: 60 }, sleepInhibited: false, sleepError: null, notificationError: null });
const connection = { state: "listening", endpoint: "ws://127.0.0.1:17373/extension", profileLabel: null, extensionVersion: null, error: null };

beforeEach(() => {
  vi.mocked(writeClipboardText).mockReset().mockResolvedValue();
  call.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_system_preferences") return initial;
    if (command === "save_system_preferences") return { ...initial, preferences: (args as { preferences: unknown }).preferences };
    if (command === "get_browser_extension_status" || command === "revoke_browser_extension") return connection;
    if (command === "prepare_browser_extension") return { path: "/app/browser-extension", connectionCode: "private-setup-code" };
    if (command === "open_browser_extension_directory" || command === "open_browser_application") return;
    throw new Error(`Unexpected command: ${command}`);
  });
});

it.each(["embedded", "extension"])("shows integrated navigation and optional extension setup with legacy %s preferences", async mode => {
  const original = call.getMockImplementation();
  call.mockImplementation(async (command, args, options) => command === "get_system_preferences"
    ? { ...initial, preferences: { ...initial.preferences, browser: { mode, application: "chrome" } } }
    : original?.(command, args, options));
  render(<BrowserSettings />);
  expect(await screen.findByText("Navegação integrada")).toBeVisible();
  expect(screen.queryByRole("combobox", { name: "Modo de navegação" })).not.toBeInTheDocument();
  expect(screen.getByText(/Os agentes abrem novas páginas dentro do Jarvis/)).toBeVisible();
  expect(screen.getByText(/A conexão não altera a navegação integrada/)).toBeVisible();
  expect(screen.getByRole("combobox", { name: "Navegador da extensão" })).toHaveTextContent("Google Chrome");
  expect(screen.getAllByRole("combobox")).toHaveLength(1);
  expect(screen.getByRole("button", { name: "Preparar extensão" })).toBeEnabled();
  expect(await screen.findByText("Aguardando extensão")).toBeVisible();
  await waitFor(() => expect(call).toHaveBeenCalledWith("get_browser_extension_status"));
  expect(call.mock.calls.filter(([command]) => command === "save_system_preferences")).toHaveLength(0);
});

it("offers Firefox with its own temporary installation and launch instructions", async () => {
  const user = userEvent.setup();
  const original = call.getMockImplementation();
  call.mockImplementation(async (command, args, options) => command === "get_system_preferences"
    ? { ...initial, preferences: { ...initial.preferences, browser: { mode: "extension", application: "firefox" } } }
    : command === "prepare_browser_extension"
      ? { path: "/app/browser-extension-firefox", connectionCode: "private-firefox-code" }
      : original?.(command, args, options));
  render(<BrowserSettings />);
  expect(await screen.findByRole("combobox", { name: "Navegador da extensão" })).toHaveTextContent("Mozilla Firefox");
  expect(screen.getByText("Carregar extensão temporária")).toBeVisible();
  expect(screen.getByText("manifest.json")).toBeVisible();
  expect(screen.getByText(/precisa ser carregada novamente ao reiniciar/)).toBeVisible();
  expect(screen.queryByText("Carregar sem compactação")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Preparar extensão" }));
  expect(await screen.findByText("/app/browser-extension-firefox")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Copiar endereço" }));
  expect(writeClipboardText).toHaveBeenCalledWith("about:debugging#/runtime/this-firefox");
  await user.click(screen.getByRole("button", { name: "Abrir Mozilla Firefox" }));
  expect(call).toHaveBeenCalledWith("open_browser_application", { application: "firefox" });
});

it("saves the extension application without changing agent navigation or other preferences and clears the old prepared package", async () => {
  const user = userEvent.setup();
  let browser = { mode: "extension", application: "chrome" };
  const original = call.getMockImplementation();
  call.mockImplementation(async (command, args, options) => {
    if (command === "get_system_preferences") return { ...initial, preferences: { ...initial.preferences, browser } };
    if (command === "save_system_preferences") {
      browser = (args as { preferences: { browser: typeof browser } }).preferences.browser;
      return { ...initial, preferences: { ...initial.preferences, browser } };
    }
    return original?.(command, args, options);
  });
  render(<BrowserSettings />);
  await user.click(await screen.findByRole("button", { name: "Preparar extensão" }));
  expect(await screen.findByText("/app/browser-extension")).toBeVisible();
  screen.getByRole("combobox", { name: "Navegador da extensão" }).focus();
  await user.keyboard("{Enter}");
  await user.click(await screen.findByRole("option", { name: "Mozilla Firefox" }));
  await waitFor(() => expect(screen.getByRole("combobox", { name: "Navegador da extensão" })).toHaveTextContent("Mozilla Firefox"));
  expect(browser).toEqual({ mode: "embedded", application: "firefox" });
  expect(call).toHaveBeenCalledWith("save_system_preferences", { preferences: { ...initial.preferences, browser: { mode: "embedded", application: "firefox" } } });
  expect(screen.queryByText("/app/browser-extension")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Copiar código de conexão" })).toBeDisabled();
});

it("guides unpacked installation, copies private setup only on request and revokes the code", async () => {
  const user = userEvent.setup();
  const original = call.getMockImplementation();
  call.mockImplementation(async (command, args, options) => command === "get_system_preferences"
    ? { ...initial, preferences: { ...initial.preferences, browser: { mode: "extension", application: "edge" } } }
    : original?.(command, args, options));
  render(<BrowserSettings />);
  expect(await screen.findByText("Carregar sem compactação")).toBeVisible();
  expect(screen.getByRole("button", { name: "Copiar código de conexão" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Preparar extensão" }));
  expect(await screen.findByText("/app/browser-extension")).toBeVisible();
  expect(screen.queryByText("private-setup-code")).not.toBeInTheDocument();
  expect(writeClipboardText).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Copiar código de conexão" }));
  expect(writeClipboardText).toHaveBeenCalledWith("private-setup-code");
  await user.click(screen.getByRole("button", { name: "Abrir pasta" }));
  expect(call).toHaveBeenCalledWith("open_browser_extension_directory");
  await user.click(screen.getByRole("button", { name: "Abrir Microsoft Edge" }));
  expect(call).toHaveBeenCalledWith("open_browser_application", { application: "edge" });
  await user.click(screen.getByRole("button", { name: "Revogar conexão" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Copiar código de conexão" })).toBeDisabled());
});

it("does not overlap status requests while a connection check is pending", async () => {
  vi.useFakeTimers();
  let finish: ((value: unknown) => void) | undefined;
  const original = call.getMockImplementation();
  call.mockImplementation(async (command, args, options) => {
    if (command === "get_system_preferences") return { ...initial, preferences: { ...initial.preferences, browser: { mode: "extension", application: "chrome" } } };
    if (command === "get_browser_extension_status") return new Promise(resolve => { finish = resolve; });
    return original?.(command, args, options);
  });
  const rendered = render(<BrowserSettings />);
  try {
    await act(async () => { await Promise.resolve(); });
    fireEvent.click(screen.getByRole("button", { name: "Atualizar conexão" }));
    fireEvent.click(screen.getByRole("button", { name: "Atualizar conexão" }));
    await act(async () => { vi.advanceTimersByTime(20_000); });
    expect(call.mock.calls.filter(([command]) => command === "get_browser_extension_status")).toHaveLength(1);
    await act(async () => { finish?.(connection); });
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(call.mock.calls.filter(([command]) => command === "get_browser_extension_status")).toHaveLength(2);
  } finally { rendered.unmount(); vi.useRealTimers(); }
});
