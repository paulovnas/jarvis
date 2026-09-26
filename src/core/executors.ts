import { z } from "zod";

export const executorSchema = z.enum(["jarvis", "claude"]);
export type Executor = z.infer<typeof executorSchema>;
export type ExecutionChoice = { executor?: Executor; account: string; model: string; reasoning: string | null };
export type ExecutionSelection = { executor?: Executor; model: string; reasoning: string | null };
export const executorOf = (choice?: { executor?: Executor } | null): Executor => choice?.executor ?? "jarvis";
export const executionLabel = (choice: ExecutionChoice) => `${executorOf(choice) === "claude" ? "Claude Code" : choice.account} / ${choice.model}`;

export function executionSelection(choice?: ExecutionChoice | null): ExecutionSelection | null {
  return choice ? { executor: executorOf(choice), model: executorOf(choice) === "claude" ? choice.model : `${choice.account}/${choice.model}`, reasoning: choice.reasoning } : null;
}

export function executionChoice(selection: ExecutionSelection): ExecutionChoice {
  if (executorOf(selection) === "claude") return { executor: "claude", account: "", model: selection.model, reasoning: selection.reasoning };
  const split = selection.model.indexOf("/");
  return { executor: "jarvis", account: split < 0 ? "" : selection.model.slice(0, split), model: split < 0 ? selection.model : selection.model.slice(split + 1), reasoning: selection.reasoning };
}

export const claudeRuntimeSchema = z.object({
  installed: z.boolean(), authenticated: z.boolean(), version: z.string().nullable(),
  models: z.array(z.object({ id: z.string(), name: z.string(), description: z.string(), reasoningLevels: z.array(z.string()), defaultReasoning: z.string().nullable() })),
  error: z.string().nullable(), authMethod: z.string().nullish(), email: z.string().nullish(), subscriptionType: z.string().nullish(),
});
export type ClaudeRuntime = z.infer<typeof claudeRuntimeSchema>;
export function claudeModels(runtime: ClaudeRuntime | null) {
  return runtime?.models.map(model => ({ value: model.id, label: model.name, reasoningLevels: model.reasoningLevels, defaultReasoningLevel: model.defaultReasoning })) ?? [];
}
