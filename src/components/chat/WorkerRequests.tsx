import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { ROLE_LABELS, type WorkflowAgent } from "@/core/workflow";
import { libraryError } from "@/core/library";
import { questionKey, type QuestionDraft } from "@/core/questions";
import { ToolApproval } from "./ToolApproval";
import { QuestionCard } from "./QuestionCard";

export function WorkerRequests({ conversationId, projectPath, agents, drafts }: { conversationId: string; projectPath: string; agents: WorkflowAgent[]; drafts: Map<string, QuestionDraft> }) {
  const agent = agents.find(agent => agent.pendingApproval || agent.pendingQuestion);
  if (!agent) return null;
  const answer = async (command: string, args: Record<string, unknown>) => {
    try { await invoke(command, { conversationId, agentId: agent.id, turnId: agent.activeTurnId, ...args }); return true; }
    catch (cause) { toast.error(libraryError(cause, "Não foi possível responder ao agente.")); return false; }
  };
  return <section aria-label={`Solicitação de ${ROLE_LABELS[agent.role]}`} className="mb-2">
    <p className="micro-label mb-2 px-1 text-primary">{ROLE_LABELS[agent.role]} · {agent.title}</p>
    {agent.pendingApproval && <ToolApproval key={agent.pendingApproval.id} tool={agent.pendingApproval} projectPath={projectPath} onAnswer={approved => answer("approve_workflow_tool", { toolId: agent.pendingApproval?.id, approved })} />}
    {agent.pendingQuestion && <QuestionCard key={agent.pendingQuestion.toolId} request={agent.pendingQuestion} drafts={drafts} draftKey={questionKey(`${conversationId}/${agent.id}`, agent.pendingQuestion)} onAnswer={async (request, response) => { const accepted = await answer("answer_workflow_question", { turnId: request.turnId, toolId: request.toolId, response }); if (accepted) drafts.delete(questionKey(`${conversationId}/${agent.id}`, request)); return accepted; }} />}
  </section>;
}
