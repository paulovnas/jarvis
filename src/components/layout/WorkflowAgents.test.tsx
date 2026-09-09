import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WorkflowAgents } from "./WorkflowAgents";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { ROLE_COLORS, ROLE_LABELS, type WorkflowAgent } from "@/core/workflow";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const agent: WorkflowAgent = { id:"worker1",parentId:"main",role:"builder",title:"Implementar busca",status:"running",createdAt:1,updatedAt:2,startedAt:1,durationMs:2_000,currentThought:"Conferindo o contrato atual",attempts:1,options:{account:"personal",model:"gpt-5.6-terra",reasoning:"high",mode:"build",workflow:"planned",approvalMode:"manual"},beadId:"task1",handoff:null,error:null,activeTurnId:"turn1",pendingApproval:null,pendingQuestion:null };
beforeEach(() => { vi.mocked(invoke).mockReset(); });
it("keeps distinct built-in role colors on waiting cards consistent with Settings", () => {
  const roles = (Object.keys(ROLE_COLORS) as WorkflowAgent["role"][]).filter(role => role !== "custom");
  const agents = roles.map(role => ({ ...agent, id: role, role, status: "waiting" as const }));
  render(<WorkflowAgents conversationId="c1" workflow={{ data: { conversationId: "c1", revision: 1, flow: "complete", agents }, error: null, loading: false, retry: vi.fn() }} />);
  const colors = roles.map(role => {
    const card = screen.getByRole("button", { name: `Abrir agente ${ROLE_LABELS[role]}: ${agent.title}` });
    expect(card).toHaveStyle({ borderColor: `${ROLE_COLORS[role]}80` });
    return getComputedStyle(card).borderColor;
  });
  expect(new Set(colors).size).toBe(roles.length);
});
it("shows the account suffix, model and effort and updates execution states with the current roster", () => {
  const current = { ...agent, options: { ...agent.options, account: "openai-codex-paulo" } };
  const workflow = { data: { conversationId: "c1", revision: 1, flow: "planned" as const, agents: [current] }, error: null, loading: false, retry: vi.fn() };
  const view = render(<WorkflowAgents conversationId="c1" workflow={workflow} />);
  const card = screen.getByRole("button", { name: "Abrir agente Construtor: Implementar busca" });
  expect(within(card).getByText("paulo")).toBeInTheDocument();
  expect(card).toHaveTextContent("gpt-5.6-terra·Alto");
  expect(card).toHaveAttribute("data-status", "running");
  view.rerender(<WorkflowAgents conversationId="c1" workflow={{ ...workflow, data: { ...workflow.data, agents: [{ ...current, status: "waiting" }] } }} />);
  expect(card).toHaveAttribute("data-status", "waiting");
  expect(within(card).getByText("Aguardando")).toBeInTheDocument();
  expect(within(card).queryByLabelText("Em execução")).not.toBeInTheDocument();
  view.rerender(<WorkflowAgents conversationId="c1" workflow={{ ...workflow, data: { ...workflow.data, agents: [] } }} />);
  expect(screen.queryByRole("button", { name: /Abrir agente/ })).not.toBeInTheDocument();
});
it("keeps only the latest card when a role runs more than once", () => {
  const first = { ...agent, id: "designer-first", role: "designer" as const, title: "Primeira análise", createdAt: 10, updatedAt: 20, status: "completed" as const };
  const latest = { ...agent, id: "designer-latest", role: "designer" as const, title: "Nova análise", createdAt: 30, updatedAt: 30, status: "running" as const };
  render(<WorkflowAgents conversationId="c1" workflow={{ data: { conversationId: "c1", revision: 1, flow: "planned", agents: [first, latest] }, error: null, loading: false, retry: vi.fn() }} />);
  expect(screen.queryByRole("button", { name: "Abrir agente Designer: Primeira análise" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Abrir agente Designer: Nova análise" })).toBeInTheDocument();
});
it("shows the live thought and advances the worker duration while it runs", () => {
  vi.useFakeTimers();
  vi.setSystemTime(12_500);
  try {
    const current = { ...agent, startedAt: 10_000, durationMs: 2_000, currentThought: "Conferindo o contrato atual" };
    const workflow = { data: { conversationId: "c1", revision: 1, flow: "planned" as const, agents: [current] }, error: null, loading: false, retry: vi.fn() };
    const view = render(<WorkflowAgents conversationId="c1" workflow={workflow} />);
    const card = screen.getByRole("button", { name: "Abrir agente Construtor: Implementar busca" });
    expect(within(card).getByText("Conferindo o contrato atual")).toHaveClass("reasoning-shimmer");
    expect(within(card).getByLabelText("Tempo de execução")).toHaveTextContent("2s");
    act(() => vi.advanceTimersByTime(1_000));
    expect(within(card).getByLabelText("Tempo de execução")).toHaveTextContent("3s");
    view.rerender(<WorkflowAgents conversationId="c1" workflow={{ ...workflow, data: { ...workflow.data, agents: [{ ...current, status: "completed", durationMs: 3_500 }] } }} />);
    expect(within(card).getByLabelText("Tempo de execução")).toHaveTextContent("3s");
    act(() => vi.advanceTimersByTime(5_000));
    expect(within(card).getByLabelText("Tempo de execução")).toHaveTextContent("3s");
    view.unmount();
  } finally {
    vi.useRealTimers();
  }
});
it("does not count time in the queue as execution time", () => {
  const queued = { ...agent, status: "queued" as const, startedAt: 1, durationMs: 0, currentThought: null };
  render(<WorkflowAgents conversationId="c1" workflow={{ data: { conversationId: "c1", revision: 1, flow: "planned", agents: [queued] }, error: null, loading: false, retry: vi.fn() }} />);
  const card = screen.getByRole("button", { name: "Abrir agente Construtor: Implementar busca" });
  expect(within(card).getByLabelText("Tempo de execução")).toHaveTextContent("0s");
});
it("loads a read-only transcript only after clicking an agent card", async () => {
  const user = userEvent.setup(); const turn = savedTurn(); turn.user = "Implementar busca"; turn.steps[0].text = "Busca implementada com validação.";
  vi.mocked(invoke).mockResolvedValue({ ...emptyChat(),conversationId:"worker1",turns:[turn] });
  render(<WorkflowAgents conversationId="c1" workflow={{data:{conversationId:"c1",revision:1,flow:"planned",agents:[agent]},error:null,loading:false,retry:vi.fn()}} />);
  expect(invoke).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button",{name:"Abrir agente Construtor: Implementar busca"}));
  expect(await screen.findByText("Busca implementada com validação.")).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith("get_workflow_transcript",{conversationId:"c1",agentId:"worker1"});
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(screen.queryByRole("button",{name:/Enviar|Autorizar/})).not.toBeInTheDocument();
});

it("keeps every custom step visible even when steps use the same agent role", () => {
  const agents = [{ ...agent, id: "custom-first", role: "custom" as const, title: "1. Analista", status: "completed" as const }, { ...agent, id: "custom-second", role: "custom" as const, title: "2. Revisor" }];
  render(<WorkflowAgents conversationId="c1" workflow={{ data: { conversationId: "c1", revision: 1, flow: "custom", agents }, error: null, loading: false, retry: vi.fn() }} />);
  expect(screen.getByRole("button", { name: "Abrir agente Customizado: 1. Analista" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Abrir agente Customizado: 2. Revisor" })).toBeVisible();
});

it("shows the frozen custom identity and opens the correct step transcript", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockResolvedValue({ ...emptyChat(), conversationId: "worker1", turns: [] });
  const custom = { ...agent, role: "custom" as const, title: "1. Analista", identity: { name: "Analista de segurança", appearance: { icon: "shield" as const, color: "purple" as const } } };
  render(<WorkflowAgents conversationId="c1" workflow={{ data: { conversationId: "c1", revision: 1, flow: "custom", agents: [custom] }, error: null, loading: false, retry: vi.fn() }} />);
  const card = screen.getByRole("button", { name: "Abrir agente Analista de segurança: 1. Analista" });
  expect(card.querySelector("svg.lucide-shield-check")).toHaveStyle({ color: "var(--color-onedark-purple)" });
  await user.click(card);
  expect(await screen.findByRole("dialog")).toHaveTextContent("Analista de segurança");
  expect(invoke).toHaveBeenCalledWith("get_workflow_transcript", { conversationId: "c1", agentId: "worker1" });
});
