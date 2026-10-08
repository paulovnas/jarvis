import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PluginMcpAuth } from "./PluginMcpAuth";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const disconnected = { authenticated: false, state: "disconnected", error: null };
const connected = { authenticated: true, state: "connected", error: null };
const mocked = vi.mocked(invoke);
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }

describe("PluginMcpAuth", () => {
  beforeEach(() => { vi.clearAllMocks(); mocked.mockReset().mockResolvedValue(disconnected); });
  it("opens the browser only after connecting and keeps credentials native", async () => {
    const pending = deferred<typeof connected>(); const busy = vi.fn(); const user = userEvent.setup();
    mocked.mockImplementation(async command => command === "start_mcp_oauth" ? { flowId: "flow", authorizationUrl: "https://service.test/authorize" } : command === "wait_mcp_oauth" ? pending.promise : disconnected);
    render(<PluginMcpAuth serverId="plugin-mcp:server" name="Calendário" disabled={false} onBusyChange={busy} />);
    const connect = await screen.findByRole("button", { name: "Conectar conta" }); await waitFor(() => expect(connect).toBeEnabled());
    expect(openUrl).not.toHaveBeenCalled();
    await user.click(connect);
    expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://service.test/authorize");
    expect(mocked).toHaveBeenCalledWith("start_mcp_oauth", { id: "plugin-mcp:server" });
    expect(mocked).toHaveBeenCalledWith("wait_mcp_oauth", { flowId: "flow" });
    expect(screen.getByText("Aguardando navegador")).toBeVisible(); expect(busy).toHaveBeenLastCalledWith(true);
    await act(async () => pending.resolve(connected));
    expect(await screen.findByRole("button", { name: "Desconectar conta" })).toBeVisible(); expect(screen.getByText("Conectado")).toBeVisible();
    expect(busy).toHaveBeenLastCalledWith(false);
  });
  it("cancels a pending browser authorization and allows retry", async () => {
    const pending = deferred<typeof disconnected>(); const user = userEvent.setup();
    mocked.mockImplementation(async command => command === "start_mcp_oauth" ? { flowId: "flow", authorizationUrl: "https://service.test/authorize" } : command === "wait_mcp_oauth" ? pending.promise : disconnected);
    render(<PluginMcpAuth serverId="plugin-mcp:server" name="Calendário" disabled={false} onBusyChange={vi.fn()} />);
    const connect = await screen.findByRole("button", { name: "Conectar conta" }); await waitFor(() => expect(connect).toBeEnabled()); await user.click(connect);
    await user.click(screen.getByRole("button", { name: "Cancelar conexão" }));
    expect(mocked).toHaveBeenCalledWith("cancel_mcp_oauth", { flowId: "flow" });
    await act(async () => pending.resolve(disconnected));
    expect(await screen.findByRole("button", { name: "Conectar conta" })).toBeEnabled();
  });
  it("disconnects the account while preserving the MCP definition", async () => {
    const user = userEvent.setup(); mocked.mockImplementation(async command => command === "mcp_oauth_status" ? connected : disconnected);
    render(<PluginMcpAuth serverId="plugin-mcp:server" name="Calendário" disabled={false} onBusyChange={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "Desconectar conta" }));
    expect(mocked).toHaveBeenCalledWith("disconnect_mcp_oauth", { id: "plugin-mcp:server" });
    expect(await screen.findByRole("button", { name: "Conectar conta" })).toBeEnabled();
    expect(mocked).not.toHaveBeenCalledWith("delete_mcp_server", expect.anything());
  });
  it("cancels a flow returned after the component unmounts without opening the browser", async () => {
    const begin = deferred<{ flowId: string; authorizationUrl: string }>(); const user = userEvent.setup();
    mocked.mockImplementation(async command => command === "start_mcp_oauth" ? begin.promise : disconnected);
    const { unmount } = render(<PluginMcpAuth serverId="plugin-mcp:server" name="Calendário" disabled={false} onBusyChange={vi.fn()} />);
    const connect = await screen.findByRole("button", { name: "Conectar conta" }); await waitFor(() => expect(connect).toBeEnabled()); await user.click(connect); unmount();
    await act(async () => begin.resolve({ flowId: "late-flow", authorizationUrl: "https://service.test/authorize" }));
    expect(mocked).toHaveBeenCalledWith("cancel_mcp_oauth", { flowId: "late-flow" }); expect(openUrl).not.toHaveBeenCalled();
  });
});
