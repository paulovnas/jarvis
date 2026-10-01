import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ChatComposer } from "./ChatComposer";
import { builtinGithubAgent, builtinImageAgent, builtinVideoAgent, customAgent, customCatalog, customFlow } from "@/test/workflow-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const models = [{ provider: "local", models: [{ value: "local/model", label: "Model", reasoningLevels: [], defaultReasoningLevel: null }] }];
beforeEach(() => { vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? customCatalog : []); });

it("sends the chosen custom graph identity and uses a composer model without mutating built-in profiles", async () => {
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true), save = vi.fn();
  render(<ChatComposer modelGroups={models} onSendMessage={send} agentModels={{ data: {}, error: null, saving: false, save, refresh: vi.fn() }} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_workflow_catalog"));
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("option", { name: "Meu fluxo" }));
  await user.click(screen.getByRole("button", { name: "Configurações do chat" }));
  const validation = screen.getByRole("switch", { name: "Validação manual" });
  expect(validation).not.toBeChecked();
  await user.click(validation);
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Examine o projeto{Enter}");
  expect(send).toHaveBeenCalledWith("Examine o projeto", { executor: "jarvis", account: "local", model: "model", reasoning: null, mode: "build", workflow: "custom", customWorkflowId: customFlow.id, approvalMode: "yolo", manualValidation: true });
  expect(save).not.toHaveBeenCalled();
});

it("keeps missing saved flows explicit and refuses to silently execute Standard", async () => {
  vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? { revision: 3, flows: [], agents: [], builtinAgents: [], builtinFlows: [] } : []);
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
  await user.click(await screen.findByRole("option", { name: solo.name }));
  expect(screen.queryByRole("switch", { name: "Validação manual" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toBeDisabled();
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Analise este chamado{Enter}");
  expect(send).toHaveBeenCalledWith("Analise este chamado", { executor: "jarvis", account: "local", model: "specialist", reasoning: null, mode: "build", workflow: "custom", customAgentId: solo.id, approvalMode: "yolo" });
});

it("runs the mixed GitHub agent directly with its dedicated configurable model", async () => {
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true), save = vi.fn().mockResolvedValue(true);
  render(<ChatComposer modelGroups={models} onSendMessage={send}
    initialOptions={{ account: "local", model: "model", reasoning: null, mode: "build", workflow: "planned", approvalMode: "yolo", manualValidation: true, automaticPublication: { commit: true, push: true, pullRequest: true } }}
    agentModels={{ data: { "publication/github": { account: "local", model: "model", reasoning: null } }, error: null, saving: false, save, refresh: vi.fn() }} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_workflow_catalog"));
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("option", { name: builtinGithubAgent.name }));
  expect(screen.queryByRole("button", { name: "Configurações do chat" })).not.toBeInTheDocument();
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Liste os pull requests abertos{Enter}");
  expect(send).toHaveBeenCalledWith("Liste os pull requests abertos", { executor: "jarvis", account: "local", model: "model", reasoning: null, mode: "build", workflow: "custom", customAgentId: "builtin:github", approvalMode: "yolo" });
});

it.each([false, true])("disables saved behavior options in a reopened Github chat (running: %s)", async running => {
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true);
  let loadCatalog!: (catalog: typeof customCatalog) => void;
  const catalog = new Promise<typeof customCatalog>(resolve => { loadCatalog = resolve; });
  vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? catalog : []);
  render(<ChatComposer modelGroups={models} onSendMessage={send} running={running}
    initialOptions={{ account: "local", model: "model", reasoning: null, mode: "build", workflow: "custom", customAgentId: "builtin:github", approvalMode: "yolo", manualValidation: true, automaticPublication: { commit: true, push: true, pullRequest: true } }} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  expect(screen.queryByRole("button", { name: "Configurações do chat" })).not.toBeInTheDocument();
  await act(async () => loadCatalog(customCatalog));
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  expect(screen.queryByRole("button", { name: "Configurações do chat" })).not.toBeInTheDocument();
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Verifique a branch{Enter}");
  expect(send).toHaveBeenCalledWith("Verifique a branch", expect.objectContaining({ customAgentId: "builtin:github" }));
  expect(send.mock.calls[0][1]).not.toHaveProperty("manualValidation");
  expect(send.mock.calls[0][1]).not.toHaveProperty("automaticPublication");
});

it("reopens an old GitHub chat with its current configured model", async () => {
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true);
  const modelGroups = [{ provider: "local", models: [{ value: "local/gpt-6-luna", label: "GPT-6 Luna", reasoningLevels: [], defaultReasoningLevel: null }] }];
  render(<ChatComposer modelGroups={modelGroups} onSendMessage={send}
    initialOptions={{ account: "local", model: "gpt-5.6-luna", reasoning: null, mode: "build", workflow: "custom", customAgentId: "builtin:github", approvalMode: "yolo" }}
    agentModels={{ data: { "publication/github": { account: "local", model: "gpt-6-luna", reasoning: null } }, error: null, saving: false, save: vi.fn(), refresh: vi.fn() }} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("GPT-6 Luna"));
  expect(screen.queryByText(/gpt-5\.6-luna.*indisponível/)).not.toBeInTheDocument();
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Liste as PRs abertas{Enter}");
  expect(send).toHaveBeenCalledWith("Liste as PRs abertas", expect.objectContaining({ model: "gpt-6-luna", customAgentId: "builtin:github" }));
});

it("runs the renamed native video generator with its original identity, saved model and secondary", async () => {
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true), save = vi.fn();
  const profile = { account: "local", model: "model", reasoning: null, fallback: { account: "local", model: "model", reasoning: null } };
  vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? { ...customCatalog, builtinAgents: [builtinVideoAgent] } : []);
  render(<ChatComposer modelGroups={models} onSendMessage={send} agentModels={{ data: { "video/video": profile }, error: null, saving: false, save, refresh: vi.fn() }} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_workflow_catalog"));
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("option", { name: "Gerador de vídeos" }));
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Model");
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Crie a abertura{Enter}");
  expect(send).toHaveBeenCalledWith("Crie a abertura", { executor: "jarvis", account: "local", model: "model", reasoning: null, mode: "build", workflow: "custom", customAgentId: "builtin:video", approvalMode: "yolo" });
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("menuitem", { name: "local" }));
  const model = await screen.findByRole("menuitem", { name: "Model" });
  model.focus();
  await user.keyboard("{Enter}");
  expect(save).toHaveBeenCalledWith("video", "video", profile);
});

it("runs the image specialist with its reasoning profile independently from the image provider", async () => {
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true), save = vi.fn();
  const profile = { account: "local", model: "model", reasoning: null };
  vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? { ...customCatalog, builtinAgents: [builtinImageAgent] } : []);
  render(<ChatComposer modelGroups={models} onSendMessage={send} agentModels={{ data: { "image_generator/image_generator": profile }, error: null, saving: false, save, refresh: vi.fn() }} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_workflow_catalog"));
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("option", { name: "Gerador de imagens" }));
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Model");
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Crie quatro banners{Enter}");
  expect(send).toHaveBeenCalledWith("Crie quatro banners", { executor: "jarvis", account: "local", model: "model", reasoning: null, mode: "build", workflow: "custom", customAgentId: "builtin:image_generator", approvalMode: "yolo" });
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("menuitem", { name: "local" }));
  (await screen.findByRole("menuitem", { name: "Model" })).focus();
  await user.keyboard("{Enter}");
  expect(save).toHaveBeenCalledWith("image_generator", "image_generator", { ...profile, executor: "jarvis" });
  expect(invoke).not.toHaveBeenCalledWith("set_api_tool_config", expect.anything());
});
