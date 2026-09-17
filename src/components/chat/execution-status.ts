import { formatExecutionDuration } from "@/hooks/use-running-clock";
import { isTaskReminder } from "./tool-activity";
import type { AssistantWorkData } from "./types";
import { reasoningPreview } from "./reasoning-preview";

export function describeExecution(work: AssistantWorkData, isStreaming: boolean) {
  const allTools = work.steps.flatMap(step => step.tools);
  const waiting = allTools.some(tool => tool.name === "ask_user" && (tool.status === "running" || tool.status === "pending"));
  const tools = allTools.filter(tool => tool.name !== "ask_user" || (tool.status !== "running" && tool.status !== "pending"));
  const failures = tools.filter(tool => tool.status === "error" && !isTaskReminder(tool)).length;
  const warnings = tools.filter(isTaskReminder).length;
  const latestThinking = [...work.steps].reverse().find(step => step.thinking.trim())?.thinking ?? "";
  const preview = reasoningPreview(latestThinking);
  const retry = isStreaming ? work.retry : undefined;
  const heading = retry
    ? `Reconectando ${retry.attempt}/${retry.maxAttempts}`
    : isStreaming
      ? waiting ? "Aguardando sua resposta" : preview || "Trabalhando…"
      : `Trabalhou por ${formatExecutionDuration(work.durationSeconds * 1_000)}`;

  return { failures, heading, retry, tools, waiting, warnings };
}
