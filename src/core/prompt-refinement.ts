import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import type { ChatDraft, MessagePart } from "./chat";

export async function refinePrompt(draft: ChatDraft, conversationId?: string): Promise<ChatDraft> {
  const response = z.string().trim().min(1).max(65_536).safeParse(await invoke("refine_chat_prompt", { prompt: draft.content, conversationId: conversationId ?? null }));
  if (!response.success) throw new Error("O modelo retornou um prompt vazio ou inválido. Seu texto foi mantido.");
  const content = response.data;
  if (!draft.parts) return { content };
  const parts: MessagePart[] = [];
  let cursor = 0;
  for (const skill of draft.parts.filter(part => part.type === "skill")) {
    const token = `/${skill.name}`;
    let index = content.indexOf(token, cursor);
    while (index >= 0 && /[\p{L}\p{N}_-]/u.test(content[index + token.length] ?? "")) index = content.indexOf(token, index + token.length);
    if (index < 0) throw new Error("O refinamento não preservou as skills selecionadas. Seu texto foi mantido.");
    if (index > cursor) parts.push({ type: "text", text: content.slice(cursor, index) });
    parts.push(skill);
    cursor = index + token.length;
  }
  if (cursor < content.length) parts.push({ type: "text", text: content.slice(cursor) });
  parts.push(...draft.parts.filter(part => part.type === "attachment"));
  return { content, parts };
}
