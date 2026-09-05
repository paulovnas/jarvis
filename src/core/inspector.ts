import type { AgentTurn } from "./chat";
import type { ProviderAccount } from "./provider-accounts";

export function conversationContext(turns: AgentTurn[], accounts: ProviderAccount[] = []) {
  const current = turns[turns.length - 1];
  const model = accounts.find(account => account.alias === current?.options.account)?.models.find(item => item.id === current?.options.model);
  const limit = current?.contextWindow ?? model?.contextWindow ?? null;
  let estimatedTokens = 0;
  const estimate = (text: string) => text.length ? Math.ceil(text.length / 4) : 0;
  for (let turnIndex = turns.length - 1; turnIndex >= 0; turnIndex--) {
    const turn = turns[turnIndex];
    for (let stepIndex = turn.steps.length - 1; stepIndex >= 0; stepIndex--) {
      const step = turn.steps[stepIndex];
      // Results are appended after the response usage was measured.
      for (const tool of step.tools) {
        if (tool.status === "completed" || tool.status === "error") estimatedTokens += estimate(tool.output);
      }
      if (step.usage) {
        const { inputTokens, outputTokens } = step.usage;
        const tokens = inputTokens + outputTokens + estimatedTokens;
        return { limit, tokens, inputTokens, outputTokens, estimatedTokens, measuredModel: turn.options.model, percent: limit ? tokens / limit * 100 : null };
      }
      estimatedTokens += estimate(step.text) + estimate(step.summary);
      for (const tool of step.tools) estimatedTokens += estimate(JSON.stringify(tool.args));
    }
    estimatedTokens += estimate(turn.user);
  }
  return { limit, tokens: null, inputTokens: null, outputTokens: null, estimatedTokens: 0, measuredModel: null, percent: null };
}
