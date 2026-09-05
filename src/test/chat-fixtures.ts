import type { AgentTurn, ChatSnapshot, TurnOptions } from "@/core/chat";

export function emptyChat(id = "c1"): ChatSnapshot {
  return { conversationId: id, revision: 1, turns: [], activeTurnId: null, pendingApproval: null };
}
export const chatOptions: TurnOptions = { account: "openai-codex-pessoal", model: "model", reasoning: "medium", mode: "build", approvalMode: "manual" };
export function savedTurn(): AgentTurn {
  return { id: "turn1", createdAt: 1_735_689_600_000, durationMs: 2300, user: "Leia o README", options: chatOptions, status: "completed", steps: [{ durationMs: 2000, text: "O projeto usa **Tauri**.", summary: "Verificando o projeto.", tools: [{ id: "tool1", name: "read", args: { path: "README.md" }, status: "completed", output: "# Jarvis", durationMs: 20 }], usage: { inputTokens: 150, outputTokens: 40 } }], error: null };
}
