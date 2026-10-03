import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { Options } from "./options";

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() }, Toaster: () => null }));

type Listener = (changes: Record<string, chrome.storage.StorageChange>, area: string) => void;
const chromeMock = {
  runtime: { sendMessage: vi.fn() },
  storage: { onChanged: { addListener: vi.fn<(listener: Listener) => void>(), removeListener: vi.fn() } },
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("chrome", chromeMock);
  chromeMock.runtime.sendMessage.mockImplementation(async (message: { type: string }) => message.type === "status"
    ? { status: { state: "disconnected", message: "Navegador desconectado." } }
    : { ok: true });
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

it("shows the Jarvis identity and updates the connection status without reconnecting", async () => {
  const { unmount } = render(<Options />);
  expect(screen.getByRole("img", { name: "Jarvis" })).toHaveAttribute("src", "./icons/64.png");
  expect(screen.queryByText(/URLs das abas disponíveis/)).not.toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Navegador desconectado."));
  expect(screen.getByRole("button", { name: "Conectar" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Desconectar" })).toBeDisabled();
  const listener = chromeMock.storage.onChanged.addListener.mock.calls[0][0];
  act(() => listener({ connectionStatus: { newValue: { state: "connected", message: "Conexão ativa neste computador." } } }, "session"));
  expect(screen.getByText("Conectado", { exact: true })).toBeVisible();
  expect(screen.getByRole("status")).toHaveTextContent("Conexão ativa neste computador.");
  expect(screen.getByRole("button", { name: "Desconectar" })).toBeEnabled();
  expect(chromeMock.runtime.sendMessage).toHaveBeenCalledExactlyOnceWith({ type: "status" });
  unmount();
  expect(chromeMock.storage.onChanged.removeListener).toHaveBeenCalledWith(listener);
});

it("connects using the complete code and keeps both actions locked until it finishes", async () => {
  const user = userEvent.setup();
  render(<Options />);
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Navegador desconectado."));
  let finish!: (value: { ok: boolean }) => void;
  chromeMock.runtime.sendMessage.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const pairing = { version: 1, endpoint: "ws://127.0.0.1:17373/extension", token: "a".repeat(64) };
  const input = screen.getByRole("textbox", { name: "Código de conexão" });
  await user.click(input);
  await user.paste(JSON.stringify(pairing));
  await user.click(screen.getByRole("button", { name: "Conectar" }));
  expect(chromeMock.runtime.sendMessage).toHaveBeenLastCalledWith({ type: "connect", pairing });
  expect(input).toBeDisabled();
  expect(screen.getByRole("button", { name: "Conectar" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Desconectar" })).toBeDisabled();
  await act(async () => finish({ ok: true }));
  expect(input).toHaveValue("");
  expect(input).toBeEnabled();
  expect(toast.success).toHaveBeenCalledWith("Conexão configurada");
});

it("keeps invalid code editable and disconnects through its separate action", async () => {
  const user = userEvent.setup();
  chromeMock.runtime.sendMessage.mockResolvedValue({ status: { state: "connected", message: "Conectado." } });
  render(<Options />);
  await screen.findByText("Conectado", { exact: true });
  await user.type(screen.getByRole("textbox", { name: "Código de conexão" }), "código incompleto");
  await user.click(screen.getByRole("button", { name: "Conectar" }));
  expect(toast.error).toHaveBeenCalledWith("Código inválido. Copie o código completo nas configurações do Jarvis.");
  expect(chromeMock.runtime.sendMessage).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("textbox", { name: "Código de conexão" })).toHaveValue("código incompleto");
  chromeMock.runtime.sendMessage.mockResolvedValue({ ok: true });
  await user.click(screen.getByRole("button", { name: "Desconectar" }));
  expect(chromeMock.runtime.sendMessage).toHaveBeenLastCalledWith({ type: "disconnect" });
  expect(toast.success).toHaveBeenCalledWith("Navegador desconectado");
});

it("configures Firefox through browser APIs without claiming a debugger connection", async () => {
  const firefox = { ...chromeMock, runtime: { ...chromeMock.runtime, getURL: () => "moz-extension://profile/" } };
  vi.stubGlobal("browser", firefox);
  const user = userEvent.setup();
  render(<Options />);
  await screen.findByText(/No Firefox, permita acesso aos sites/);
  expect(screen.queryByText(/exibe seu aviso de depuração/)).not.toBeInTheDocument();
  const pairing = { version: 1, endpoint: "ws://127.0.0.1:17373/extension", token: "a".repeat(64) };
  await user.click(screen.getByRole("textbox", { name: "Código de conexão" }));
  await user.paste(JSON.stringify(pairing));
  await user.click(screen.getByRole("button", { name: "Conectar" }));
  expect(firefox.runtime.sendMessage).toHaveBeenLastCalledWith({ type: "connect", pairing });
});

it("explains Firefox data transfer before pairing without adding another approval", async () => {
  vi.stubGlobal("browser", { ...chromeMock, runtime: { ...chromeMock.runtime, getURL: () => "moz-extension://profile/" } });
  render(<Options />);
  expect(screen.getByText(/URLs das abas disponíveis/)).toHaveTextContent("conteúdo, interações e diagnósticos das páginas conectadas");
  expect(screen.getByText(/URLs das abas disponíveis/)).toHaveTextContent("podem ser processados pelos provedores de IA configurados no Jarvis");
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Navegador desconectado."));
  expect(screen.getAllByRole("button").map(button => button.textContent)).toEqual(["Conectar", "Desconectar"]);
});
