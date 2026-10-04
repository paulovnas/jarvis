import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { writeClipboardText } from "@/core/clipboard";
import { useSelfDevelopment } from "@/hooks/use-self-development";
import { ProjectSelfDevelopmentSettings } from "./ProjectSelfDevelopmentSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/core/clipboard", () => ({ writeClipboardText: vi.fn().mockResolvedValue(undefined) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const call = vi.mocked(invoke);
const source = { id: "source", projectId: "ordinary", projectName: "Meu aplicativo", title: "Erro no teste", status: "error" };
const incident = { id: "incident", capturedAt: 1000, conversationTitle: source.title, sourceProjectName: source.projectName, sourceStatus: source.status, eventCount: 3, truncated: false, reference: "Investigue o incidente de autodesenvolvimento incident usando as ferramentas autorizadas deste projeto." };
let enabled: boolean;
let incidents: typeof incident[];
function Settings() {
  const controller = useSelfDevelopment("jarvis");
  return <ProjectSelfDevelopmentSettings controller={controller} />;
}

beforeEach(() => {
  enabled = true; incidents = [];
  vi.mocked(writeClipboardText).mockClear();
  vi.mocked(toast.success).mockClear();
  call.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_self_development_status") return { projectId: "jarvis", eligible: true, enabled };
    if (command === "set_self_development_enabled") { enabled = (args as { enabled: boolean }).enabled; if (!enabled) incidents = []; return { projectId: "jarvis", eligible: true, enabled }; }
    if (command === "list_self_development_sources") return [source];
    if (command === "list_self_development_incidents") return [...incidents];
    if (command === "capture_self_development_incident") { incidents = [incident]; return incident; }
    if (command === "delete_self_development_incident") { incidents = []; return; }
    throw new Error(`Unexpected command ${command}`);
  });
});

it("activates only by explicit choice, persists native state and revokes incidents when disabled", async () => {
  enabled = false;
  const user = userEvent.setup();
  const view = render(<Settings />);
  const toggle = await screen.findByRole("switch", { name: "Ambiente de desenvolvimento do Jarvis" });
  expect(toggle).not.toBeChecked();
  expect(screen.queryByText("Investigar com o Jarvis")).not.toBeInTheDocument();
  expect(call).not.toHaveBeenCalledWith("list_self_development_sources", expect.anything());
  await user.click(toggle);
  await waitFor(() => expect(call).toHaveBeenCalledWith("set_self_development_enabled", { projectId: "jarvis", enabled: true }));
  expect(await screen.findByRole("button", { name: "Preparar incidente" })).toBeDisabled();
  expect(call).not.toHaveBeenCalledWith("capture_self_development_incident", expect.anything());
  view.unmount();
  render(<Settings />);
  expect(await screen.findByRole("switch")).toBeChecked();
  await waitFor(() => expect(screen.getByRole("switch")).toBeEnabled());
  await user.click(screen.getByRole("switch"));
  expect(call).not.toHaveBeenCalledWith("set_self_development_enabled", { projectId: "jarvis", enabled: false });
  await user.click(screen.getByRole("button", { name: "Desativar e revogar" }));
  await waitFor(() => expect(screen.getByRole("switch")).not.toBeChecked());
  expect(screen.queryByText("Investigar com o Jarvis")).not.toBeInTheDocument();
});

it("requires a selected conversation, prepares its bounded incident and copies only the prepared reference", async () => {
  const user = userEvent.setup(); render(<Settings />);
  const prepare = await screen.findByRole("button", { name: "Preparar incidente" });
  expect(prepare).toBeDisabled();
  expect(screen.getByRole("combobox", { name: "Conversa de origem" })).toHaveTextContent("Escolha uma conversa");
  await user.click(screen.getByRole("combobox", { name: "Conversa de origem" }));
  await user.click(await screen.findByRole("option", { name: "Erro no teste · Meu aplicativo · Erro" }));
  await user.type(screen.getByRole("textbox", { name: "O que aconteceu? (opcional)" }), "A ferramenta falhou.");
  expect(call).not.toHaveBeenCalledWith("capture_self_development_incident", expect.anything());
  await user.click(prepare);
  await waitFor(() => expect(call).toHaveBeenCalledWith("capture_self_development_incident", { projectId: "jarvis", conversationId: "source", description: "A ferramenta falhou." }));
  expect(await screen.findByText("3 eventos")).toBeVisible();
  expect(screen.getByText(/Credenciais, texto bruto/)).toBeVisible();
  expect(screen.getByRole("combobox", { name: "Conversa de origem" })).toHaveTextContent("Escolha uma conversa");
  await user.click(screen.getByRole("button", { name: "Copiar referência para o chat" }));
  expect(writeClipboardText).toHaveBeenCalledWith(incident.reference);
  expect(toast.success).toHaveBeenCalledWith(expect.stringContaining("Cole no chat deste projeto Jarvis"));
  expect(call.mock.calls.some(([command]) => command.includes("send_message") || command.includes("create_conversation"))).toBe(false);
});

it("clears the explicit source and description without capturing anything", async () => {
  const user = userEvent.setup(); render(<Settings />);
  await screen.findByRole("button", { name: "Preparar incidente" });
  await user.click(screen.getByRole("combobox", { name: "Conversa de origem" }));
  await user.click(await screen.findByRole("option", { name: /Erro no teste/ }));
  await user.type(screen.getByRole("textbox", { name: "O que aconteceu? (opcional)" }), "Descrição local");
  await user.click(screen.getByRole("button", { name: "Limpar seleção" }));
  expect(screen.getByRole("combobox", { name: "Conversa de origem" })).toHaveTextContent("Escolha uma conversa");
  expect(screen.getByRole("textbox", { name: "O que aconteceu? (opcional)" })).toHaveValue("");
  expect(screen.getByRole("button", { name: "Preparar incidente" })).toBeDisabled();
  expect(call).not.toHaveBeenCalledWith("capture_self_development_incident", expect.anything());
});

it("removes an incident only after its explicit revocation action", async () => {
  incidents = [incident];
  const user = userEvent.setup(); render(<Settings />);
  await user.click(await screen.findByRole("button", { name: "Revogar e remover" }));
  expect(call).not.toHaveBeenCalledWith("delete_self_development_incident", expect.anything());
  const dialog = screen.getByRole("alertdialog", { name: "Revogar este incidente?" });
  await user.click(within(dialog).getByRole("button", { name: "Revogar incidente" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("delete_self_development_incident", { projectId: "jarvis", incidentId: "incident" }));
  expect(screen.queryByRole("button", { name: "Copiar referência para o chat" })).not.toBeInTheDocument();
});

it("keeps a failed capture local, sanitizes errors and never retries automatically", async () => {
  call.mockImplementation(async command => {
    if (command === "get_self_development_status") return { projectId: "jarvis", eligible: true, enabled: true };
    if (command === "list_self_development_sources") return [source];
    if (command === "list_self_development_incidents") return [];
    throw new Error("Authorization: secret-api-key /private/raw-chat.jsonl");
  });
  const user = userEvent.setup(); render(<Settings />);
  await screen.findByRole("button", { name: "Preparar incidente" });
  await user.click(screen.getByRole("combobox", { name: "Conversa de origem" }));
  await user.click(await screen.findByRole("option", { name: /Erro no teste/ }));
  await user.type(screen.getByRole("textbox", { name: "O que aconteceu? (opcional)" }), "Rascunho preservado");
  await user.click(screen.getByRole("button", { name: "Preparar incidente" }));
  expect(await screen.findByText(/Não foi possível preparar o incidente/)).toBeVisible();
  expect(screen.getByRole("textbox", { name: "O que aconteceu? (opcional)" })).toHaveValue("Rascunho preservado");
  expect(screen.queryByText(/secret-api-key/)).not.toBeInTheDocument();
  expect(call.mock.calls.filter(([command]) => command === "capture_self_development_incident")).toHaveLength(1);
});
