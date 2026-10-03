import { chatAgentModelKey } from "@/core/chat-models";
import { executionChoice, executionSelection, executorOf } from "@/core/executors";
import { defaultReasoning, selectableReasoningLevels } from "@/core/reasoning";
import type { TurnOptions } from "@/core/chat";
import type { ModelChoice } from "@/core/provider-references";
import { ROLE_LABELS } from "@/core/workflow";
import { flowOptions, flowSelection, type FlowSelection } from "@/core/workflow-catalog";
import type { RemoteChoices } from "./client";

export function remoteModelChoice(choices: RemoteChoices, flow: FlowSelection, options: TurnOptions | null): ModelChoice | null {
  const key = chatAgentModelKey(flow);
  const historical = options && flowSelection(options) === flow ? options.modelSelection ?? executionChoice(executionSelection(options)!) : null;
  if (choices.overrides[key] || historical) return choices.overrides[key] ?? historical;
  const selected = flowOptions(flow);
  const customFlow = choices.catalog.flows.find(item => item.id === selected.customWorkflowId);
  const agentId = selected.customAgentId ?? customFlow?.steps.find(step => step.id === customFlow.entry)?.agentId;
  const agent = choices.catalog.agents.find(item => item.id === agentId);
  const builtinProfile = agentId === "builtin:github" ? "publication/github" : agentId === "builtin:video" ? "video/video" : agentId === "builtin:image_generator" ? "image_generator/image_generator" : null;
  const configured = agent?.model ?? choices.defaults[builtinProfile ?? key];
  if (configured) return configured;
  if (options) return options.modelSelection ?? executionChoice(executionSelection(options)!);
  const group = choices.models.find(item => item.models.length > 0);
  const model = group?.models[0];
  return model ? executionChoice({ executor: group?.executor, model: model.value, reasoning: defaultReasoning(model) }) : null;
}

export function remoteModelProblem(choices: RemoteChoices, flow: FlowSelection, choice: ModelChoice | null): string | null {
  const selected = flowOptions(flow);
  if (selected.customAgentId) {
    const agent = [...choices.catalog.agents, ...choices.catalog.builtinAgents].find(item => item.id === selected.customAgentId);
    if (!agent || agent.usage === "flow_only") return "Este agente não está disponível. Selecione outro agente ou fluxo.";
  }
  if (selected.customWorkflowId && !choices.catalog.flows.some(item => item.id === selected.customWorkflowId)) return "Este fluxo não está disponível. Selecione outro fluxo.";
  const available = (target: ModelChoice) => {
    const selection = executionSelection(target)!;
    const model = choices.models.filter(group => executorOf(group) === executorOf(selection)).flatMap(group => group.models).find(item => item.value === selection.model);
    return model && (!target.reasoning || selectableReasoningLevels(model.reasoningLevels).includes(target.reasoning));
  };
  if (!choice || !available(choice)) return "O modelo deste chat não está disponível. Selecione um provedor e modelo.";
  if (choice.fallback && !available(choice.fallback)) return "O modelo secundário não está disponível. Revise a seleção deste chat.";
  const customFlow = choices.catalog.flows.find(item => item.id === selected.customWorkflowId);
  const configured = customFlow
    ? choices.catalog.agents.flatMap(agent => agent.model && customFlow.steps.some(step => step.agentId === agent.id) ? [{ name: agent.name, choice: agent.model }] : [])
    : selected.workflow !== "custom"
      ? Object.entries({ ...choices.defaults, ...choices.overrides }).flatMap(([key, model]) => key.startsWith(`${selected.workflow}/`) && key !== chatAgentModelKey(flow)
        ? [{ name: ROLE_LABELS[key.split("/")[1] as keyof typeof ROLE_LABELS] ?? key, choice: model }] : [])
      : [];
  const invalidAgent = configured.find(agent => !available(agent.choice) || Boolean(agent.choice.fallback && !available(agent.choice.fallback)));
  if (invalidAgent) return `O agente ${invalidAgent.name} usa um modelo indisponível. Selecione outro fluxo ou revise a configuração desse agente.`;
  return null;
}

export function remoteTurnOptions(flow: FlowSelection, choice: ModelChoice, previous: TurnOptions | null): TurnOptions {
  const { executor, account, model, reasoning } = choice;
  const selected = flowOptions(flow);
  const options: TurnOptions = { ...previous, executor, account, model, reasoning, ...selected, customWorkflowId: selected.customWorkflowId ?? null, customAgentId: selected.customAgentId ?? null, mode: previous?.mode ?? "build", approvalMode: previous?.approvalMode ?? "yolo" };
  delete options.modelSelection;
  if (!["planned", "complete"].includes(selected.workflow ?? "") && !selected.customWorkflowId) delete options.manualValidation;
  if (selected.customAgentId === "builtin:github") delete options.automaticPublication;
  return options;
}
