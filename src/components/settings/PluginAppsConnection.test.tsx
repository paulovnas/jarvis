import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PluginAppsConnection } from "./PluginAppsConnection";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const mocked = vi.mocked(invoke);
const connectUrl = "https://chatgpt.com/apps/google-drive/connector-one";
const props = { serverId: "plugin-app:gateway", connectUrl, name: "Google Drive", active: true, disabled: false, onBusyChange: vi.fn() };

describe("PluginAppsConnection", () => {
  beforeEach(() => { vi.clearAllMocks(); mocked.mockReset().mockResolvedValue({ toolCount: 2, tools: ["search", "read"], error: null }); vi.mocked(openUrl).mockReset().mockResolvedValue(); });
  it("opens authorization only on explicit click without claiming the app is connected", async () => {
    const user = userEvent.setup(); render(<PluginAppsConnection {...props} />);
    expect(screen.getByText("Conexão ainda não verificada")).toBeVisible(); expect(mocked).not.toHaveBeenCalled(); expect(openUrl).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Autorizar no ChatGPT" }));
    expect(openUrl).toHaveBeenCalledExactlyOnceWith(connectUrl); expect(mocked).not.toHaveBeenCalled(); expect(screen.queryByText("Gateway verificado")).not.toBeInTheDocument();
  });
  it("shows the real gateway result without claiming authorization for each sibling connector", async () => {
    const user = userEvent.setup(); render(<PluginAppsConnection {...props} />);
    await user.click(screen.getByRole("button", { name: "Testar conexão" }));
    expect(mocked).toHaveBeenCalledExactlyOnceWith("test_mcp_server", { id: props.serverId });
    expect(await screen.findByText("Gateway verificado")).toBeVisible(); expect(screen.getByText("2 ferramentas disponíveis nos apps deste plugin.")).toBeVisible();
    expect(screen.getByText("A autorização de cada conector será confirmada ao usar suas ferramentas.")).toBeVisible(); expect(openUrl).not.toHaveBeenCalled();
  });
  it("shows native errors and permits an explicit retry", async () => {
    mocked.mockResolvedValueOnce({ toolCount: 0, tools: [], error: "Autorize Google Drive na conta selecionada." }); const user = userEvent.setup(); render(<PluginAppsConnection {...props} />);
    await user.click(screen.getByRole("button", { name: "Testar conexão" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Autorize Google Drive"); expect(screen.getByText("Conexão não confirmada")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Testar conexão" }));
    expect(await screen.findByText("Gateway verificado")).toBeVisible(); expect(screen.queryByRole("alert")).not.toBeInTheDocument(); expect(mocked).toHaveBeenCalledTimes(2);
  });
  it("does not equate an empty gateway inventory with an authorized service", async () => {
    mocked.mockResolvedValueOnce({ toolCount: 0, tools: [], error: null }); const user = userEvent.setup(); render(<PluginAppsConnection {...props} />);
    await user.click(screen.getByRole("button", { name: "Testar conexão" }));
    expect(await screen.findByText("Nenhuma ferramenta disponível")).toBeVisible(); expect(screen.queryByText("Gateway verificado")).not.toBeInTheDocument(); expect(screen.getByText(/O gateway respondeu, mas/)).toBeVisible();
  });
  it("blocks duplicate tests and holds settings busy until the native test finishes", async () => {
    let resolve!: (value: unknown) => void; const pending = new Promise<unknown>(done => { resolve = done; }); mocked.mockReturnValueOnce(pending);
    const onBusyChange = vi.fn(); const user = userEvent.setup(); render(<PluginAppsConnection {...props} onBusyChange={onBusyChange} />);
    await user.dblClick(screen.getByRole("button", { name: "Testar conexão" }));
    expect(mocked).toHaveBeenCalledTimes(1); expect(onBusyChange).toHaveBeenLastCalledWith(true); expect(screen.getByRole("button", { name: "Autorizar no ChatGPT" })).toBeDisabled();
    await act(async () => resolve({ toolCount: 1, tools: ["read"], error: null }));
    await waitFor(() => expect(onBusyChange).toHaveBeenLastCalledWith(false)); expect(screen.getByText("1 ferramenta disponível nos apps deste plugin.")).toBeVisible();
  });
  it("requires an active available account for testing while leaving authorization accessible", async () => {
    render(<PluginAppsConnection {...props} active={false} />);
    expect(screen.getByRole("button", { name: "Testar conexão" })).toBeDisabled(); expect(screen.getByRole("button", { name: "Autorizar no ChatGPT" })).toBeEnabled(); expect(screen.getByText(/selecione uma conta ChatGPT disponível/)).toBeVisible(); expect(mocked).not.toHaveBeenCalled();
  });
  it("reports browser opening failures without creating a connected state", async () => {
    vi.mocked(openUrl).mockRejectedValueOnce(new Error("opener failed")); const user = userEvent.setup(); render(<PluginAppsConnection {...props} />);
    await user.click(screen.getByRole("button", { name: "Autorizar no ChatGPT" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível abrir a autorização"); expect(screen.queryByText("Gateway verificado")).not.toBeInTheDocument();
  });
});
