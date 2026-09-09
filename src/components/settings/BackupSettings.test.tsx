import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { ProviderAccount } from "@/core/provider-accounts";
import { BackupSettings } from "./BackupSettings";

const { invokeMock, openMock, saveMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  openMock: vi.fn(),
  saveMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openMock, save: saveMock }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const summary = { customAgents: 1, customFlows: 2, skills: 3, mcps: 1, modelTargets: 1 };
const preview = {
  fingerprint: `sha256:${"a".repeat(64)}`,
  createdAt: 1_757_376_000,
  appVersion: "0.9.1-beta",
  archiveBytes: 2_048,
  summary,
  modelTargets: [{ id: "builtin:planned/planner", kind: "builtin_agent", label: "Planejador", details: ["Fluxo Planejado", "Agente Jarvis"] }],
  warnings: [
    "Provedores, contas, credenciais de IA e modelos não fazem parte do backup.",
    "Configurações de MCP podem conter chaves de acesso. Guarde este ZIP em local seguro.",
  ],
};
const account: ProviderAccount = {
  alias: "openai-codex-paulo",
  providerKind: "openai-codex",
  enabled: true,
  createdAt: 1,
  email: "paulo@example.com",
  accountType: "personal",
  modelsAvailable: true,
  models: [{ id: "gpt-5.6-sol", name: "GPT-5.6 Sol", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" }],
};

describe("settings backup", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    openMock.mockReset();
    saveMock.mockReset();
    vi.mocked(toast.success).mockReset();
    vi.mocked(toast.error).mockReset();
  });

  it("exports to the user-selected ZIP and reports the saved file", async () => {
    const user = userEvent.setup();
    saveMock.mockResolvedValue("/tmp/Jarvis-backup.zip");
    invokeMock.mockResolvedValue({ path: "/tmp/Jarvis-backup.zip", bytes: 2048, summary });
    render(<BackupSettings accounts={[account]} />);

    expect(screen.getByText(/MCPs podem incluir chaves de acesso/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Escolher destino" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("export_settings_backup", { path: "/tmp/Jarvis-backup.zip" }));
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({ filters: [{ name: "Backup do Jarvis", extensions: ["zip"] }] }));
    expect(toast.success).toHaveBeenCalledWith("Backup criado", expect.objectContaining({ description: expect.stringContaining("2 KB") }));
  });

  it("inspects first, explains exclusions and restores with an explicit model mapping", async () => {
    const user = userEvent.setup();
    openMock.mockResolvedValue("/tmp/portable.zip");
    invokeMock.mockImplementation((command: string) => Promise.resolve(command === "inspect_settings_backup" ? preview : { summary, mappedModels: 1 }));
    render(<BackupSettings accounts={[account]} />);

    await user.click(screen.getByRole("button", { name: "Escolher arquivo" }));
    const review = await screen.findByRole("dialog", { name: "Revisar backup" });
    expect(within(review).getByText("3")).toBeVisible();
    expect(within(review).getByText(/Provedores, contas, credenciais de IA/)).toBeVisible();
    expect(within(review).getByText(/MCP podem conter chaves/)).toBeVisible();

    await user.click(within(review).getByRole("button", { name: "Revisar modelos" }));
    const mapping = screen.getByRole("dialog", { name: "Associar modelos" });
    await user.click(within(mapping).getByRole("combobox", { name: "Provedor para Planejador" }));
    await user.click(await screen.findByRole("option", { name: account.alias }));
    expect(within(mapping).getByRole("combobox", { name: "Modelo para Planejador" })).toHaveTextContent("GPT-5.6 Sol");
    expect(within(mapping).getByRole("combobox", { name: "Raciocínio para Planejador" })).toHaveTextContent("Alto");
    await user.click(within(mapping).getByRole("button", { name: "Restaurar configurações" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("import_settings_backup", {
      path: "/tmp/portable.zip",
      fingerprint: preview.fingerprint,
      mappings: [{ targetId: "builtin:planned/planner", choice: { account: account.alias, model: "gpt-5.6-sol", reasoning: "high" } }],
    }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Associar modelos" })).not.toBeInTheDocument());
    expect(toast.success).toHaveBeenCalledWith("Configurações restauradas", expect.objectContaining({ description: expect.stringContaining("Provedores e históricos foram preservados") }));
  });

  it("keeps current settings untouched when inspection fails", async () => {
    const user = userEvent.setup();
    openMock.mockResolvedValue("/tmp/invalid.zip");
    invokeMock.mockRejectedValue({ message: "O arquivo escolhido não foi criado pelo Jarvis." });
    render(<BackupSettings accounts={[]} />);

    await user.click(screen.getByRole("button", { name: "Escolher arquivo" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("não foi criado pelo Jarvis");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
