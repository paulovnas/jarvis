import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PluginMcpConfiguration } from "./PluginMcpConfiguration";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));
const mocked = vi.mocked(invoke);
describe("PluginMcpConfiguration", () => {
  beforeEach(() => { vi.clearAllMocks(); mocked.mockReset().mockResolvedValue({ fields: ["API_KEY", "WORKSPACE"], configured: false }); });
  it("shows required field names but never retrieves stored credential values", async () => {
    const user = userEvent.setup(); render(<PluginMcpConfiguration serverId="plugin-mcp:server" name="Monday" active disabled={false} onBusyChange={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "Configurar credenciais" }));
    const key = screen.getByLabelText("API_KEY"); expect(key).toHaveAttribute("type", "password"); expect(key).toHaveValue("");
    expect(screen.getByLabelText("WORKSPACE")).toHaveValue("");
    expect(mocked).toHaveBeenCalledExactlyOnceWith("plugin_mcp_requirements", { id: "plugin-mcp:server" });
    await user.click(screen.getByRole("button", { name: "Salvar credenciais" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Preencha todos os campos");
    expect(mocked).not.toHaveBeenCalledWith("configure_plugin_mcp", expect.anything());
  });
  it("saves only after explicit submit, blocks close while pending and clears local inputs", async () => {
    let resolve!: (value: unknown) => void; const pending = new Promise<unknown>(done => { resolve = done; }); const busy = vi.fn(); const user = userEvent.setup();
    mocked.mockImplementation(command => command === "configure_plugin_mcp" ? pending : Promise.resolve({ fields: ["API_KEY", "WORKSPACE"], configured: false }));
    render(<PluginMcpConfiguration serverId="plugin-mcp:server" name="Monday" active disabled={false} onBusyChange={busy} />);
    await user.click(await screen.findByRole("button", { name: "Configurar credenciais" }));
    fireEvent.change(screen.getByLabelText("API_KEY"), { target: { value: "local-secret" } }); fireEvent.change(screen.getByLabelText("WORKSPACE"), { target: { value: "space" } });
    expect(mocked).not.toHaveBeenCalledWith("configure_plugin_mcp", expect.anything()); await user.click(screen.getByRole("button", { name: "Salvar credenciais" }));
    expect(mocked).toHaveBeenCalledWith("configure_plugin_mcp", { id: "plugin-mcp:server", values: { API_KEY: "local-secret", WORKSPACE: "space" } }); expect(busy).toHaveBeenLastCalledWith(true);
    await user.keyboard("{Escape}"); expect(screen.getByRole("dialog", { name: "Configurar Monday" })).toBeVisible();
    await act(async () => resolve({ fields: ["API_KEY", "WORKSPACE"], configured: true }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument()); expect(busy).toHaveBeenLastCalledWith(false);
    await user.click(screen.getByRole("button", { name: "Alterar credenciais" })); expect(screen.getByLabelText("API_KEY")).toHaveValue("");
  });
  it("does not query inactive plugin MCPs", () => {
    render(<PluginMcpConfiguration serverId="plugin-mcp:server" name="Monday" active={false} disabled={false} onBusyChange={vi.fn()} />);
    expect(screen.getByText(/Ative o plugin/)).toBeVisible(); expect(mocked).not.toHaveBeenCalled();
  });
});
