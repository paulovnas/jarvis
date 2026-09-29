import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { DEFAULT_HTTP_SETTINGS, type HttpSettings } from "@/core/http-client";
import { ProjectHttpSettings } from "./ProjectHttpSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const call = vi.mocked(invoke);
const initial: HttpSettings = {
  ...DEFAULT_HTTP_SETTINGS, projectId: "p1", revision: 3,
  variables: [{ id: "url", name: "base_url", value: "http://localhost:3000", secret: false, enabled: true, configured: false }, { id: "token", name: "token", value: "", secret: true, enabled: true, configured: true }],
  environments: [{ id: "dev", name: "Desenvolvimento", color: "green", variables: [] }],
};

beforeEach(() => {
  vi.clearAllMocks();
  call.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_project_http_settings") return initial;
    if (command === "save_project_http_settings") return { ...(args as { settings: HttpSettings }).settings, revision: 4 };
    if (command === "export_project_http") return;
    if (command === "import_project_http") return { ...initial, revision: 4, environments: [] };
    throw new Error(`Unexpected command ${command}`);
  });
});

it("saves shared and environment variables independently and preserves a stored secret", async () => {
  const user = userEvent.setup();
  render(<ProjectHttpSettings projectId="p1" />);
  expect(await screen.findByRole("textbox", { name: "Valor da variável 1 · Projeto" })).toHaveValue("http://localhost:3000");
  expect(screen.getByLabelText("Valor da variável 2 · Projeto")).toHaveValue("");
  expect(screen.getByLabelText("Valor da variável 2 · Projeto")).toHaveAttribute("type", "password");
  expect(screen.getByPlaceholderText("Segredo salvo · deixe vazio para manter")).toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: "Cor do ambiente 1" })).toHaveTextContent("Verde");
  expect(screen.getByLabelText("Tempo total (segundos)")).toHaveValue(null);
  await user.click(screen.getByRole("button", { name: "Adicionar variável · Desenvolvimento" }));
  await user.type(screen.getByLabelText("Nome da variável 1 · Desenvolvimento"), "base_url");
  await user.type(screen.getByLabelText("Valor da variável 1 · Desenvolvimento"), "http://localhost:4000");
  await user.click(screen.getByRole("button", { name: "Salvar cliente HTTP" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_http_settings", { projectId: "p1", settings: expect.objectContaining({
    revision: 3, variables: initial.variables,
    environments: [{ ...initial.environments[0], variables: [expect.objectContaining({ name: "base_url", value: "http://localhost:4000", secret: false })] }],
    defaults: expect.objectContaining({ totalTimeoutSeconds: null }),
  }) }));
  expect(toast.success).toHaveBeenCalledWith("Configurações HTTP salvas");
});

it("keeps secret state independent from environment scope and allows clearing stored credentials", async () => {
  const user = userEvent.setup();
  render(<ProjectHttpSettings projectId="p1" />);
  await user.click(await screen.findByRole("button", { name: "Adicionar variável · Desenvolvimento" }));
  await user.type(screen.getByLabelText("Nome da variável 1 · Desenvolvimento"), "api_key");
  await user.click(screen.getByRole("switch", { name: /Secreta.*Variável 1 · Desenvolvimento/ }));
  await user.type(screen.getByLabelText("Valor da variável 1 · Desenvolvimento"), "test-credential");
  await user.click(screen.getByRole("button", { name: "Limpar segredo salvo" }));
  await user.click(screen.getByRole("button", { name: "Salvar cliente HTTP" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_http_settings", { projectId: "p1", settings: expect.objectContaining({
    variables: [initial.variables[0], { ...initial.variables[1], value: "", configured: false }],
    environments: [{ ...initial.environments[0], variables: [expect.objectContaining({ name: "api_key", secret: true, value: "test-credential" })] }],
  }) }));
});

it("retains unsaved edits on revision conflicts and only discards them after explicit reload", async () => {
  call.mockImplementation(async command => {
    if (command === "get_project_http_settings") return initial;
    if (command === "save_project_http_settings") throw { code: "http_revision_conflict", message: "A configuração foi alterada em outro lugar." };
    throw new Error(command);
  });
  const user = userEvent.setup();
  render(<ProjectHttpSettings projectId="p1" />);
  const name = await screen.findByLabelText("Nome do ambiente 1");
  await user.clear(name); await user.type(name, "Homologação");
  await user.click(screen.getByRole("button", { name: "Salvar cliente HTTP" }));
  expect(await screen.findByText("Configuração não salva")).toBeVisible();
  expect(name).toHaveValue("Homologação");
  expect(call.mock.calls.filter(([command]) => command === "get_project_http_settings")).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "Recarregar configuração" }));
  await user.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(name).toHaveValue("Homologação");
  await user.click(screen.getByRole("button", { name: "Recarregar configuração" }));
  await user.click(screen.getByRole("button", { name: "Recarregar" }));
  expect(await screen.findByLabelText("Nome do ambiente 1")).toHaveValue("Desenvolvimento");
});

it("exports only the saved configuration through the native file command", async () => {
  vi.mocked(save).mockResolvedValue("/tmp/http-config.json");
  const user = userEvent.setup();
  render(<ProjectHttpSettings projectId="p1" />);
  await user.click(await screen.findByRole("button", { name: "Exportar configuração" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("export_project_http", { projectId: "p1", path: "/tmp/http-config.json" }));
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ filters: [{ name: "Configuração HTTP do Jarvis", extensions: ["json"] }] }));
  await user.type(screen.getByLabelText("Nome do ambiente 1"), " alterado");
  expect(screen.getByRole("button", { name: "Exportar configuração" })).toBeDisabled();
  expect(screen.getByText("Salve suas alterações antes de exportar.")).toBeVisible();
});

it("confirms replacement before import and sends the revision without a force overwrite", async () => {
  vi.mocked(open).mockResolvedValue("/tmp/http-config.json");
  const user = userEvent.setup();
  render(<ProjectHttpSettings projectId="p1" />);
  await user.click(await screen.findByRole("button", { name: "Importar configuração" }));
  const dialog = await screen.findByRole("alertdialog", { name: "Importar configuração HTTP?" });
  expect(within(dialog).getByText(/segredos não vêm no arquivo/)).toBeVisible();
  expect(call).not.toHaveBeenCalledWith("import_project_http", expect.anything());
  await user.click(within(dialog).getByRole("button", { name: "Importar" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("import_project_http", { projectId: "p1", path: "/tmp/http-config.json", revision: 3 }));
  expect(await screen.findByText("Nenhum ambiente")).toBeVisible();
});

it("recovers an initial load failure and keeps transport changes explicit", async () => {
  call.mockRejectedValueOnce(new Error("Disconnected"));
  const user = userEvent.setup();
  render(<ProjectHttpSettings projectId="p1" />);
  await user.click(await screen.findByRole("button", { name: "Tentar novamente" }));
  await user.click(await screen.findByRole("button", { name: "Transporte avançado" }));
  expect(screen.getByRole("switch", { name: "Verificar certificado TLS" })).toBeChecked();
  await user.click(screen.getByRole("switch", { name: "Verificar certificado TLS" }));
  expect(screen.getByText("Verificação TLS desativada")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Adicionar ambiente" }));
  expect(screen.getByLabelText("Nome do ambiente 2")).toHaveValue("Novo ambiente");
  await user.click(screen.getByRole("button", { name: "Remover ambiente 2" }));
  expect(screen.queryByLabelText("Nome do ambiente 2")).not.toBeInTheDocument();
  expect(call.mock.calls.filter(([command]) => command === "save_project_http_settings")).toHaveLength(0);
});
