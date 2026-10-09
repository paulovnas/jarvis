import { useState, type ComponentProps } from "react";
import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Button } from "@/components/ui/button";
import type { ChatDraft, MessagePart } from "@/core/chat";
import { SkillInput } from "./SkillInput";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
type HistoryPage = { messages: string[]; before: number };
type LoadHistory = NonNullable<ComponentProps<typeof SkillInput>["loadHistory"]>;

function Harness({ loadHistory, initialDraft = { content: "" }, onChange = vi.fn() }: { loadHistory: LoadHistory; initialDraft?: ChatDraft; onChange?: (draft: ChatDraft) => void }) {
  const [draft, setDraft] = useState(initialDraft);
  const files = draft.parts?.filter(part => part.type === "attachment") ?? [];
  return <>
    <SkillInput draft={draft} onChange={next => { setDraft(next); onChange(next); }} onSend={vi.fn()} disabled={false} compacting={false} working={false} loadHistory={loadHistory} attachments={files.map(part => <span key={part.attachment.id}>{part.attachment.name}</span>)}>
      <Button onClick={() => setDraft({ content: "Rascunho externo" })}>Restaurar rascunho</Button>
    </SkillInput>
  </>;
}

async function openInput(loadHistory: LoadHistory, initialDraft?: ChatDraft, onChange?: (draft: ChatDraft) => void) {
  const result = render(<Harness loadHistory={loadHistory} initialDraft={initialDraft} onChange={onChange} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  const user = userEvent.setup();
  await user.click(field);
  return { ...result, field, user };
}

async function arrow(field: HTMLElement, key: "ArrowUp" | "ArrowDown", text: string) {
  fireEvent.keyDown(field, { key });
  await waitFor(() => expect(field.textContent).toBe(text));
}

describe("composer keyboard history", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("moves through previous messages and returns to the empty draft without crossing either bound", async () => {
    const load = vi.fn<LoadHistory>().mockResolvedValue({ messages: ["Primeira", "Segunda", "Mais recente"], before: 0 });
    const { field } = await openInput(load);
    await arrow(field, "ArrowDown", "");
    expect(load).not.toHaveBeenCalled();
    await arrow(field, "ArrowUp", "Mais recente");
    await arrow(field, "ArrowUp", "Segunda");
    await arrow(field, "ArrowUp", "Primeira");
    await arrow(field, "ArrowUp", "Primeira");
    await arrow(field, "ArrowDown", "Segunda");
    await arrow(field, "ArrowDown", "Mais recente");
    await arrow(field, "ArrowDown", "");
    await arrow(field, "ArrowDown", "");
    expect(load).toHaveBeenCalledExactlyOnceWith(undefined);
  });

  it("fetches older pages at the boundary and passes over a page with no user text", async () => {
    const load = vi.fn<LoadHistory>()
      .mockResolvedValueOnce({ messages: ["Mais recente"], before: 4 })
      .mockResolvedValueOnce({ messages: [], before: 2 })
      .mockResolvedValueOnce({ messages: ["Mais antiga"], before: 0 });
    const { field } = await openInput(load);
    await arrow(field, "ArrowUp", "Mais recente");
    expect(load).toHaveBeenCalledTimes(1);
    await arrow(field, "ArrowUp", "Mais antiga");
    expect(load.mock.calls).toEqual([[undefined], [4], [2]]);
    await arrow(field, "ArrowDown", "Mais recente");
    expect(load).toHaveBeenCalledTimes(3);
  });

  it("stops navigation after typing or pasting so the edited message cannot be replaced", async () => {
    const load = vi.fn<LoadHistory>().mockResolvedValue({ messages: ["Mensagem anterior", "Mensagem recente"], before: 0 });
    const { field, user } = await openInput(load);
    await arrow(field, "ArrowUp", "Mensagem recente");
    await user.keyboard(" editada");
    await arrow(field, "ArrowUp", "Mensagem recente editada");
    await arrow(field, "ArrowDown", "Mensagem recente editada");
    expect(load).toHaveBeenCalledTimes(1);
    await user.keyboard("{Control>}a{/Control}{Backspace}");
    await arrow(field, "ArrowUp", "Mensagem recente");
    await user.paste(" colada");
    await arrow(field, "ArrowDown", "Mensagem recente colada");
    await arrow(field, "ArrowUp", "Mensagem recente colada");
    expect(load).toHaveBeenCalledTimes(2);
  });

  it.each(["edit", "external draft", "composition"] as const)("does not overwrite %s while a history request is pending", async interruption => {
    let resolve!: (page: HistoryPage) => void;
    const load = vi.fn<LoadHistory>(() => new Promise(done => { resolve = done; }));
    const changed = vi.fn();
    const { field, user } = await openInput(load, undefined, changed);
    fireEvent.keyDown(field, { key: "ArrowUp" });
    expect(load).toHaveBeenCalledExactlyOnceWith(undefined);
    if (interruption === "edit") await user.type(field, "Texto em edição");
    else if (interruption === "external draft") await user.click(screen.getByRole("button", { name: "Restaurar rascunho" }));
    else fireEvent.compositionStart(field);
    const expected = interruption === "edit" ? "Texto em edição" : interruption === "external draft" ? "Rascunho externo" : "";
    const count = changed.mock.calls.length;
    await act(async () => resolve({ messages: ["Mensagem atrasada"], before: 0 }));
    expect(field.textContent).toBe(expected);
    expect(changed).toHaveBeenCalledTimes(count);
    if (interruption === "composition") fireEvent.compositionEnd(field);
  });

  it("ignores a pending history result after closing the old input", async () => {
    let resolve!: (page: HistoryPage) => void;
    const load = vi.fn<LoadHistory>(() => new Promise(done => { resolve = done; }));
    const oldChanged = vi.fn();
    const old = await openInput(load, undefined, oldChanged);
    fireEvent.keyDown(old.field, { key: "ArrowUp" });
    const oldCount = oldChanged.mock.calls.length;
    old.unmount();
    const changed = vi.fn();
    const { field } = await openInput(vi.fn<LoadHistory>().mockResolvedValue({ messages: ["Outro chat"], before: 0 }), undefined, changed);
    const count = changed.mock.calls.length;
    await act(async () => resolve({ messages: ["Chat fechado"], before: 0 }));
    expect(field.textContent).toBe("");
    expect(oldChanged).toHaveBeenCalledTimes(oldCount);
    expect(changed).toHaveBeenCalledTimes(count);
  });

  it("preserves ordinary cursor navigation for modifiers, composition and selected text", async () => {
    const load = vi.fn<LoadHistory>().mockResolvedValue({ messages: ["Primeira", "Mais recente"], before: 0 });
    const { field, user } = await openInput(load);
    for (const modifier of ["altKey", "ctrlKey", "metaKey", "shiftKey"] as const) fireEvent.keyDown(field, { key: "ArrowUp", [modifier]: true });
    fireEvent.keyDown(field, { key: "ArrowUp", isComposing: true });
    expect(load).not.toHaveBeenCalled();
    await arrow(field, "ArrowUp", "Mais recente");
    await user.keyboard("{Control>}a{/Control}");
    await arrow(field, "ArrowUp", "Mais recente");
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("preserves current attachments while recalling text and when returning to the empty draft", async () => {
    const file: MessagePart = { type: "attachment", attachment: { id: "image", conversationId: "c1", name: "atual.png", mime: "image/png", size: 100, kind: "image" } };
    const changed = vi.fn();
    const load = vi.fn<LoadHistory>().mockResolvedValue({ messages: ["/review Revise esta imagem"], before: 0 });
    const { field } = await openInput(load, { content: "", parts: [file] }, changed);
    await arrow(field, "ArrowUp", "/review Revise esta imagem");
    expect(changed).toHaveBeenLastCalledWith({ content: "/review Revise esta imagem", parts: [{ type: "text", text: "/review Revise esta imagem" }, file] });
    expect(screen.getByText("atual.png")).toBeVisible();
    expect(screen.queryByRole("button", { name: /Remover skill/ })).not.toBeInTheDocument();
    await arrow(field, "ArrowDown", "");
    expect(changed).toHaveBeenLastCalledWith({ content: "", parts: expect.arrayContaining([file]) });
    expect(screen.getByText("atual.png")).toBeVisible();
  });

  it("lets the skill suggestion menu own arrows before message history", async () => {
    const skill = { id: "first", name: "primeira", description: "Primeira skill", origin: "project", path: "/skills/first", enabled: true, automatic: true, source: null, marketplaceId: null, updateAvailable: false, updateError: null };
    vi.mocked(invoke).mockResolvedValue({ includeAgents: false, directory: "/skills", skills: [skill, { ...skill, id: "second", name: "segunda" }], warnings: [] });
    const load = vi.fn<LoadHistory>().mockResolvedValue({ messages: ["Mensagem recente"], before: 0 });
    const { field, user } = await openInput(load);
    await user.type(field, "/");
    await screen.findByRole("option", { name: /primeira/ });
    await user.keyboard("{ArrowDown}{Tab}");
    expect(screen.getByRole("button", { name: "Remover skill segunda" })).toBeVisible();
    expect(load).not.toHaveBeenCalled();
  });
});
