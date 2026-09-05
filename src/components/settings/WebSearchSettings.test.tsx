import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { ProviderAccount } from "@/core/provider-accounts";
import { WebSearchSettings } from "./WebSearchSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const invokeMock = vi.mocked(invoke);
const account = (alias: string, providerKind = "openai-codex"): ProviderAccount => ({
  alias, providerKind, enabled: true, models: [], modelsAvailable: false, createdAt: 1, email: null, accountType: "personal",
});

describe("WebSearchSettings", () => {
  it("preserva a conta desativada e volta a disponibilizá-la ao reativar", async () => {
    invokeMock.mockResolvedValue({ accountAlias: "openai-codex-pesquisa" });
    const active = account("openai-codex-pesquisa");
    const view = render(<WebSearchSettings accounts={[{ ...active, enabled: false }]} />);
    expect(await screen.findByRole("combobox")).toHaveTextContent("Indisponível");
    view.rerender(<WebSearchSettings accounts={[active]} />);
    expect(screen.getByRole("combobox")).toHaveTextContent(active.alias);
    expect(screen.getByRole("combobox")).not.toHaveTextContent("Indisponível");
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
  beforeEach(() => { vi.clearAllMocks(); invokeMock.mockReset(); });

  it("começa desligado e oferece apenas contas de provedores compatíveis", async () => {
    invokeMock.mockResolvedValue({ accountAlias: null });
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[account("openai-codex-pesquisa"), account("futuro", "unsupported")]} />);
    const select = await screen.findByRole("combobox", { name: "Conta para pesquisa" });
    expect(select).toHaveTextContent("Desligado");
    await user.click(select);
    expect(await screen.findByRole("option", { name: "openai-codex-pesquisa" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "futuro" })).not.toBeInTheDocument();
  });

  it("salva a conta própria da pesquisa, restaura ao reabrir e permite desligar", async () => {
    let selected: string | null = null;
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "set_web_search_config" && args && "accountAlias" in args) selected = args.accountAlias as string | null;
      return { accountAlias: selected };
    });
    const user = userEvent.setup();
    const view = render(<WebSearchSettings accounts={[account("openai-codex-pesquisa"), account("openai-codex-chat")]} />);
    await user.click(await screen.findByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: "openai-codex-pesquisa" }));
    expect(invokeMock).toHaveBeenCalledWith("set_web_search_config", { accountAlias: "openai-codex-pesquisa" });
    await waitFor(() => expect(screen.getByRole("combobox")).toHaveTextContent("openai-codex-pesquisa"));
    view.unmount();
    render(<WebSearchSettings accounts={[account("openai-codex-pesquisa")]} />);
    expect(await screen.findByRole("combobox")).toHaveTextContent("openai-codex-pesquisa");
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: "Desligado" }));
    await waitFor(() => expect(screen.getByRole("combobox")).toHaveTextContent("Desligado"));
    expect(invokeMock).toHaveBeenCalledWith("set_web_search_config", { accountAlias: null });
  });

  it("bloqueia alterações simultâneas e mantém a seleção após falha ao salvar", async () => {
    let rejectSave!: (error: Error) => void;
    invokeMock.mockResolvedValueOnce({ accountAlias: null }).mockReturnValueOnce(new Promise((_, reject) => { rejectSave = reject; }));
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[account("openai-codex-pesquisa")]} />);
    await user.click(await screen.findByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: "openai-codex-pesquisa" }));
    expect(screen.getByRole("combobox")).toBeDisabled();
    await act(async () => { rejectSave(new Error("offline")); });
    expect(screen.getByRole("combobox")).toHaveTextContent("Desligado");
    expect(screen.getByRole("combobox")).toBeEnabled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining("seleção anterior"));
  });

  it("mostra falha de leitura e permite recarregar sem sobrescrever a configuração", async () => {
    invokeMock.mockRejectedValueOnce(new Error("storage")).mockResolvedValueOnce({ accountAlias: "openai-codex-pesquisa" });
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[account("openai-codex-pesquisa")]} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível carregar");
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Recarregar Web Search" }));
    expect(await screen.findByRole("combobox")).toHaveTextContent("openai-codex-pesquisa");
    expect(invokeMock).not.toHaveBeenCalledWith("set_web_search_config", expect.anything());
  });

  it("sinaliza uma conta indisponível sem selecionar outra automaticamente", async () => {
    invokeMock.mockResolvedValue({ accountAlias: "openai-codex-ausente" });
    render(<WebSearchSettings accounts={[account("openai-codex-outra")]} />);
    expect(await screen.findByRole("combobox")).toHaveTextContent("openai-codex-ausente · Indisponível");
    expect(screen.getByText(/A conta selecionada está indisponível/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
