import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WorkflowAgents } from "./WorkflowAgents";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import type { WorkflowAgent } from "@/core/workflow";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const agent: WorkflowAgent = { id:"worker1",parentId:"main",role:"builder",title:"Implementar busca",status:"running",createdAt:1,updatedAt:2,attempts:1,options:{account:"personal",model:"gpt-5.6-terra",reasoning:"high",mode:"build",workflow:"planned",approvalMode:"manual"},beadId:"task1",handoff:null,error:null,activeTurnId:"turn1",pendingApproval:null,pendingQuestion:null };
beforeEach(() => { vi.mocked(invoke).mockReset(); });
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
