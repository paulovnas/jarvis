import { invoke } from "@tauri-apps/api/core";
import { historyPageSchema, type ChatSnapshot } from "./chat";

export async function readComposerHistory(conversationId: string, snapshot: ChatSnapshot | undefined, before?: number): Promise<{ messages: string[]; before: number }> {
  if (snapshot && snapshot.conversationId !== conversationId) throw new Error("O histórico recebido não corresponde à conversa selecionada.");
  let turns = snapshot?.turns ?? [];
  let start = snapshot?.history?.start ?? 0;
  const latest = before === undefined;
  if (!latest || !snapshot || start + turns.length < (snapshot.history?.total ?? turns.length)) {
    const page = historyPageSchema.parse(await invoke<unknown>("get_chat_history", { conversationId, ...(latest ? {} : { before }) }));
    if (page.conversationId !== conversationId) throw new Error("O histórico recebido não corresponde à conversa selecionada.");
    turns = page.turns;
    start = page.history.start;
  }
  const messages = turns.flatMap(turn => [turn.user, ...(turn.auxiliaryMessages?.map(message => message.content) ?? [])]);
  if (latest) messages.push(...(snapshot?.queuedMessages?.map(message => message.content) ?? []));
  return { messages: messages.filter(message => message.trim()), before: start };
}
