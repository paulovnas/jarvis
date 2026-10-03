import { flowOptions, type FlowSelection } from "./workflow-catalog";
import { rootRole } from "./workflow";

export function chatAgentModelKey(selection: FlowSelection): string {
  const options = flowOptions(selection);
  return options.customAgentId ? `agent:${options.customAgentId}` : options.customWorkflowId ? `flow:${options.customWorkflowId}` : `${options.workflow ?? "standard"}/${rootRole(options.workflow ?? "standard")}`;
}
