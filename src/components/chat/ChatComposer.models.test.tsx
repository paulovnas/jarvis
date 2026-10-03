import type { ComponentProps } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { useChatAgentModels } from "@/hooks/use-chat-agent-models";
import type { AgentModelConfig, ModelChoice } from "@/hooks/use-agent-models";
import type { WorkflowCatalog } from "@/core/workflow-catalog";
import { chatOptions } from "@/test/chat-fixtures";
import { ChatComposer, type ProviderModelGroup } from "./ChatComposer";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/hooks/use-voice", () => ({ stopDictation: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@/components/voice/VoiceControls", () => ({ VoiceSessionPanel: () => null, VoiceControls: () => null }));
vi.mock("@/hooks/use-workflow-catalog", () => ({ useWorkflowCatalog: () => ({ data: catalog, error: null, saving: false, refresh: vi.fn(), mutate: vi.fn() }) }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => ({
  data: { installed: true, authenticated: true, version: "1", error: null,
    preferences: { enabled: true, disabledModels: [] },
    models: [{ id: "sonnet", name: "Sonnet", description: "", reasoningLevels: [], defaultReasoning: null }] },
  loading: false, error: null, refresh: vi.fn(),
}) }));

const base: ModelChoice = { account: "work", model: "base", reasoning: null };
const sol: ModelChoice = { executor: "jarvis", account: "work", model: "sol", reasoning: null };
const swift: ModelChoice = { executor: "jarvis", account: "work", model: "swift", reasoning: null };
const claude: ModelChoice = { executor: "claude", account: "", model: "sonnet", reasoning: null };
const models: ProviderModelGroup[] = [{ provider: "Pessoal", models: [
  { value: "work/base", label: "Base", reasoningLevels: [], defaultReasoningLevel: null },
  { value: "work/sol", label: "SOL 6.1", reasoningLevels: [], defaultReasoningLevel: null },
  { value: "work/swift", label: "Swift", reasoningLevels: [], defaultReasoningLevel: null },
] }];
const globalSave = vi.fn();
const globalModels = { data: { "standard/builder": base, "designer/designer": base }, error: null, saving: false, save: globalSave, refresh: vi.fn() };
let stored: Record<string, AgentModelConfig>;
let catalog: WorkflowCatalog;

beforeEach(() => {
  vi.clearAllMocks();
  stored = { "chat-a": {}, "chat-b": {} };
  catalog = { revision: 1, agents: [], flows: [], builtinAgents: [], builtinFlows: [] };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const payload = args as { conversationId: string; key: string; choice: ModelChoice };
    if (command === "set_chat_agent_model") {
      stored[payload.conversationId] = { ...stored[payload.conversationId], [payload.key]: payload.choice };
      return stored[payload.conversationId];
    }
    if (command === "get_chat_agent_models") return stored[payload.conversationId];
    return undefined;
  });
});

function Chat({ id, ...props }: Omit<ComponentProps<typeof ChatComposer>, "chatModels" | "draftKey" | "modelGroups"> & { id: string; modelGroups?: ProviderModelGroup[] }) {
  const chatModels = useChatAgentModels(id);
  return <section aria-label={id}><ChatComposer agentModels={globalModels} modelGroups={models} {...props} draftKey={id} chatModels={chatModels} /></section>;
}

async function chooseModel(user: ReturnType<typeof userEvent.setup>, chat: HTMLElement, label: string) {
  within(chat).getByRole("button", { name: "Selecionar modelo de IA" }).focus();
  await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Pessoal" })).focus();
  await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: label }));
}

async function chooseFlow(user: ReturnType<typeof userEvent.setup>, label: string) {
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("option", { name: label }));
}

it("changes only the selected conversation when two projects use the same agent", async () => {
  const sendA = vi.fn().mockResolvedValue(true), sendB = vi.fn().mockResolvedValue(true);
  render(<><Chat id="chat-a" onSendMessage={sendA} /><Chat id="chat-b" onSendMessage={sendB} /></>);
  await screen.findAllByRole("textbox", { name: "Mensagem" });
  const a = screen.getByRole("region", { name: "chat-a" }), b = screen.getByRole("region", { name: "chat-b" });
  const user = userEvent.setup();
  await waitFor(() => expect(within(a).getByRole("button", { name: "Selecionar modelo de IA" })).not.toBeDisabled());
  await chooseModel(user, a, "SOL 6.1");
  await waitFor(() => expect(within(a).getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("SOL 6.1"));
  expect(within(b).getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Base");
  expect(globalSave).not.toHaveBeenCalled();
  expect(stored["chat-a"]).toEqual({ "standard/builder": sol });
  expect(stored["chat-b"]).toEqual({});
  await user.type(within(a).getByRole("textbox", { name: "Mensagem" }), "Implemente A{Enter}");
  await user.type(within(b).getByRole("textbox", { name: "Mensagem" }), "Implemente B{Enter}");
  expect(sendA).toHaveBeenCalledWith("Implemente A", expect.objectContaining(sol));
  expect(sendB).toHaveBeenCalledWith("Implemente B", expect.objectContaining(base));
});

it("restores the saved chat model after an old Claude turn finishes and after remounting", async () => {
  stored["chat-a"] = { "standard/builder": sol };
  const initialOptions = { ...chatOptions, ...claude, workflow: "standard" as const };
  const send = vi.fn().mockResolvedValue(true);
  const view = render(<Chat id="chat-a" onSendMessage={send} initialOptions={initialOptions} running />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Sonnet"));
  view.rerender(<Chat id="chat-a" onSendMessage={send} initialOptions={initialOptions} running={false} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("SOL 6.1"));
  view.unmount();
  render(<Chat id="chat-a" onSendMessage={send} initialOptions={initialOptions} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("SOL 6.1"));
  expect(globalSave).not.toHaveBeenCalled();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "set_chat_agent_model")).toBe(false);
});

it("keeps a durable explicit choice authoritative over a matching legacy provider replacement", async () => {
  stored["chat-a"] = { "standard/builder": base };
  const user = userEvent.setup(), send = vi.fn().mockResolvedValue(true);
  render(<Chat id="chat-a" onSendMessage={send} initialOptions={{ ...chatOptions, ...base, workflow: "standard" }}
    modelBindings={[{ itemKey: "chat:chat-a", source: base, target: sol }]} />);
  const editor = await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Base"));
  await user.type(editor, "Continue com meu modelo{Enter}");
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Base");
  expect(send).toHaveBeenCalledWith("Continue com meu modelo", expect.objectContaining(base));
});

it("remembers independent local choices when switching agents and flows", async () => {
  const user = userEvent.setup();
  render(<Chat id="chat-a" onSendMessage={vi.fn().mockResolvedValue(true)} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  const chat = screen.getByRole("region", { name: "chat-a" });
  await chooseModel(user, chat, "Swift");
  await chooseFlow(user, "Designer");
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Base");
  await chooseModel(user, chat, "SOL 6.1");
  await chooseFlow(user, "Padrão");
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Swift");
  await chooseFlow(user, "Designer");
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("SOL 6.1");
  expect(stored["chat-a"]).toEqual({ "standard/builder": swift, "designer/designer": sol });
  expect(globalSave).not.toHaveBeenCalled();
});

it.each(["primary", "fallback"] as const)("blocks sending after the saved %s disappears and keeps the red picker usable", async removed => {
  const fallback = { account: "removed", model: "gone", reasoning: null };
  stored["chat-a"] = { "standard/builder": removed === "primary" ? fallback : { ...base, fallback } };
  const send = vi.fn().mockResolvedValue(true), user = userEvent.setup();
  render(<Chat id="chat-a" onSendMessage={send} />);
  const editor = await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveAttribute("aria-invalid", "true"));
  const picker = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(picker).toHaveClass("border-destructive"); expect(picker).not.toBeDisabled();
  await user.type(editor, "Meu pedido{Enter}");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  expect(send).not.toHaveBeenCalled(); expect(editor).toHaveTextContent("Meu pedido");
  await chooseModel(user, screen.getByRole("region", { name: "chat-a" }), "SOL 6.1");
  await waitFor(() => expect(picker).not.toHaveAttribute("aria-invalid"));
  expect(globalSave).not.toHaveBeenCalled();
  if (removed === "fallback") expect(stored["chat-a"]["standard/builder"].fallback).toBeNull();
  await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
  expect(send).toHaveBeenCalledWith("Meu pedido", expect.objectContaining(sol));
});

it("preserves a valid secondary model in the local override without changing the agent", async () => {
  stored["chat-a"] = { "standard/builder": { ...base, fallback: swift } };
  const user = userEvent.setup();
  render(<Chat id="chat-a" onSendMessage={vi.fn().mockResolvedValue(true)} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  await chooseModel(user, screen.getByRole("region", { name: "chat-a" }), "SOL 6.1");
  await waitFor(() => expect(stored["chat-a"]).toEqual({ "standard/builder": { ...sol, fallback: swift } }));
  expect(globalSave).not.toHaveBeenCalled();
});

it("overrides a custom agent model only inside its current conversation", async () => {
  const id = "a".repeat(32), user = userEvent.setup(), send = vi.fn().mockResolvedValue(true);
  const agent = { id, name: "Especialista", description: "", instructions: "Ajude o usuário", usage: "mixed" as const, capability: "commands" as const, model: base };
  catalog.agents = [agent];
  render(<Chat id="chat-a" onSendMessage={send} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  await chooseFlow(user, "Especialista");
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Base");
  await chooseModel(user, screen.getByRole("region", { name: "chat-a" }), "SOL 6.1");
  await waitFor(() => expect(stored["chat-a"]).toEqual({ [`agent:${id}`]: sol }));
  expect(agent.model).toEqual(base); expect(globalSave).not.toHaveBeenCalled();
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Faça o ajuste{Enter}");
  expect(send).toHaveBeenCalledWith("Faça o ajuste", expect.objectContaining({ ...sol, workflow: "custom", customAgentId: id }));
});

it("blocks a custom flow's removed local root fallback and still validates its configured child models", async () => {
  const agentId = "a".repeat(32), flowId = "b".repeat(32), entry = "c".repeat(32);
  const child = { id: agentId, name: "Especialista", description: "", instructions: "Ajude o usuário", usage: "flow_only" as const, capability: "commands" as const, model: base };
  catalog.agents = [child];
  catalog.flows = [{ id: flowId, name: "Meu fluxo", description: "", entry, maxSteps: 3,
    steps: [{ id: entry, agentId, instructions: "", position: { x: 0, y: 0 }, next: null, onRework: null }] }];
  stored["chat-a"] = { [`flow:${flowId}`]: { ...base, fallback: { account: "removed", model: "gone", reasoning: null } } };
  const send = vi.fn().mockResolvedValue(true), user = userEvent.setup();
  const initialOptions = { ...chatOptions, ...base, workflow: "custom" as const, customWorkflowId: flowId };
  const view = render(<Chat id="chat-a" onSendMessage={send} initialOptions={initialOptions} />);
  const editor = await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveAttribute("aria-invalid", "true"));
  await user.type(editor, "Execute meu fluxo{Enter}");
  expect(send).not.toHaveBeenCalled();
  expect(editor).toHaveTextContent("Execute meu fluxo");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  await chooseModel(user, screen.getByRole("region", { name: "chat-a" }), "SOL 6.1");
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).not.toHaveAttribute("aria-invalid"));
  expect(child.model).toEqual(base);
  catalog = { ...catalog, agents: [{ ...child, model: { account: "removed", model: "gone", reasoning: null } }] };
  view.rerender(<Chat id="chat-a" onSendMessage={send} initialOptions={initialOptions} />);
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  expect(editor).toHaveTextContent("Execute meu fluxo");
  expect(send).not.toHaveBeenCalled();
});

it("preserves the selected model and draft when saving another choice fails", async () => {
  const user = userEvent.setup();
  render(<Chat id="chat-a" onSendMessage={vi.fn().mockResolvedValue(true)} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Meu rascunho");
  vi.mocked(invoke).mockRejectedValueOnce({ message: "Não foi possível salvar" });
  await chooseModel(user, screen.getByRole("region", { name: "chat-a" }), "SOL 6.1");
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).not.toBeDisabled());
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Base");
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveTextContent("Meu rascunho");
  expect(globalSave).not.toHaveBeenCalled();
});
