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

it("defaults old preferences to embedded and changes browser mode without losing other preferences", async () => {
  const user = userEvent.setup();
  render(<BrowserSettings />);
  const mode = await screen.findByRole("combobox", { name: "Modo de navegação" });
  expect(mode).toHaveTextContent("Embutido no Jarvis");
  expect(call).not.toHaveBeenCalledWith("get_browser_extension_status");
  mode.focus();
  await user.keyboard("{Enter}");
  await user.click(await screen.findByRole("option", { name: "Extensão Chromium" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_system_preferences", { preferences: { ...initial.preferences, browser: { mode: "extension", application: "chrome" } } }));
  expect(await screen.findByText("Aguardando extensão")).toBeVisible();
  expect(screen.getByRole("combobox", { name: "Navegador instalado" })).toHaveTextContent("Google Chrome");
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
