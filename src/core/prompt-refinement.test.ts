import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { refinePrompt } from "./prompt-refinement";
import { draftText, type ChatDraft } from "./chat";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());

it("identifies the conversation so native refinement can use its actual context", async () => {
  vi.mocked(invoke).mockResolvedValue("Continue a implementação descrita na conversa.");
  await refinePrompt({ content: "Pode continuar" }, "c1");
  expect(invoke).toHaveBeenCalledWith("refine_chat_prompt", { prompt: "Pode continuar", conversationId: "c1" });
});

it("refines only draft text and preserves selected skills and attachments", async () => {
  const skill = { type: "skill" as const, id: "skill-1", name: "beads" };
  const attachment = { type: "attachment" as const, attachment: { id: "a1", conversationId: "c1", name: "notas.txt", kind: "document" as const, mime: "text/plain", size: 8 } };
  const draft: ChatDraft = { content: "Use /beads para planejar", parts: [{ type: "text", text: "Use " }, skill, { type: "text", text: " para planejar" }, attachment] };
  vi.mocked(invoke).mockResolvedValue("  Contexto: projeto atual.\nObjetivo: planejar com /beads, preservando as restrições.  ");
  const refined = await refinePrompt(draft);
  expect(invoke).toHaveBeenCalledWith("refine_chat_prompt", { prompt: draft.content, conversationId: null });
  expect(refined.parts).toContainEqual(skill);
  expect(refined.parts).toContainEqual(attachment);
  expect(draftText(refined.parts ?? [])).toBe(refined.content);
  expect(draft.content).toBe("Use /beads para planejar");
});

it("keeps plain drafts free of attachment or skill metadata", async () => {
  vi.mocked(invoke).mockResolvedValue("Objetivo: explicar o projeto de forma concisa.");
  expect(await refinePrompt({ content: "Explique o projeto" })).toEqual({ content: "Objetivo: explicar o projeto de forma concisa." });
});

it("rejects missing skill invocations and similarly named skills", async () => {
  vi.mocked(invoke).mockResolvedValue("Use /beads-extra para planejar.");
  await expect(refinePrompt({ content: "/beads", parts: [{ type: "skill", id: "b", name: "beads" }] })).rejects.toThrow("não preservou as skills");
});

it.each(["", "   ", null, { content: "unexpected" }])("rejects an unusable provider response without changing the draft: %j", async value => {
  const draft = { content: "Meu pedido original" };
  vi.mocked(invoke).mockResolvedValue(value);
  await expect(refinePrompt(draft)).rejects.toThrow();
  expect(draft.content).toBe("Meu pedido original");
});
