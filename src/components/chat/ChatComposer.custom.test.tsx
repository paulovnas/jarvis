import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ChatComposer } from "./ChatComposer";
import { customAgent, customCatalog, customFlow } from "@/test/workflow-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const models = [{ provider: "local", models: [{ value: "local/model", label: "Model", reasoningLevels: [], defaultReasoningLevel: null }] }];
beforeEach(() => { vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? customCatalog : []); });

it("sends the chosen custom graph identity and uses a composer model without mutating built-in profiles", async () => {
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true), save = vi.fn();
  render(<ChatComposer modelGroups={models} onSendMessage={send} agentModels={{ data: {}, error: null, saving: false, save, refresh: vi.fn() }} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_workflow_catalog"));
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("menuitem", { name: "Meu fluxo" }));
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Examine o projeto{Enter}");
  expect(send).toHaveBeenCalledWith("Examine o projeto", { account: "local", model: "model", reasoning: null, mode: "build", workflow: "custom", customWorkflowId: customFlow.id, approvalMode: "yolo" });
  expect(save).not.toHaveBeenCalled();
});

it("keeps missing saved flows explicit and refuses to silently execute Standard", async () => {
  vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? { revision: 3, flows: [], agents: [] } : []);
  const user = userEvent.setup(), send = vi.fn();
  render(<ChatComposer modelGroups={models} onSendMessage={send} initialOptions={{ account: "local", model: "model", reasoning: null, mode: "build", workflow: "custom", customWorkflowId: customFlow.id, approvalMode: "manual" }} />);
  expect(await screen.findByText(/Este fluxo foi removido/)).toBeVisible();
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Não execute outro fluxo{Enter}");
  expect(send).not.toHaveBeenCalled();
});

it("runs a Solo agent as the main chat agent with its fixed model", async () => {
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true);
  const solo = { ...customAgent, usage: "solo" as const, model: { account: "local", model: "specialist", reasoning: null } };
  vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? { ...customCatalog, agents: [solo] } : []);
  const modelGroups = [{ provider: "local", models: [
    { value: "local/general", label: "General", reasoningLevels: [], defaultReasoningLevel: null },
    { value: "local/specialist", label: "Specialist", reasoningLevels: [], defaultReasoningLevel: null },
  ] }];
  render(<ChatComposer modelGroups={modelGroups} onSendMessage={send} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_workflow_catalog"));
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("menuitem", { name: solo.name }));
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toBeDisabled();
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Analise este chamado{Enter}");
  expect(send).toHaveBeenCalledWith("Analise este chamado", { account: "local", model: "specialist", reasoning: null, mode: "build", workflow: "custom", customAgentId: solo.id, approvalMode: "yolo" });
});
