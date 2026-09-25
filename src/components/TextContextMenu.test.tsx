import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPortal } from "react-dom";
import { TextContextMenu } from "./TextContextMenu";
import { Input, Textarea } from "./TextInput";
import { SkillInput } from "./chat/SkillInput";
import { readClipboardText, writeClipboardText } from "@/core/clipboard";
import type { ChatDraft } from "@/core/chat";

vi.mock("@/core/clipboard", () => ({ readClipboardText: vi.fn(), writeClipboardText: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue({ skills: [] }) }));

beforeEach(() => {
  vi.mocked(readClipboardText).mockReset().mockResolvedValue("novo texto");
  vi.mocked(writeClipboardText).mockReset().mockResolvedValue(undefined);
  vi.stubGlobal("DataTransfer", class {
    files: File[] = [];
    data = new Map<string, string>();
    setData(type: string, text: string) { this.data.set(type, text); }
    getData(type: string) { return this.data.get(type) ?? ""; }
  });
  vi.stubGlobal("ClipboardEvent", class extends Event {
    clipboardData: DataTransfer | null;
    constructor(type: string, options: ClipboardEventInit) { super(type, options); this.clipboardData = options.clipboardData ?? null; }
  });
});
afterEach(() => { window.getSelection()?.removeAllRanges(); vi.unstubAllGlobals(); });

it("copies only the selected message text and keeps the blank background menu suppressed", async () => {
  const user = userEvent.setup();
  render(<TextContextMenu><div><p>Reutilizar esta mensagem</p><div data-testid="background" /></div></TextContextMenu>);
  fireEvent.contextMenu(screen.getByTestId("background"));
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  const message = screen.getByText("Reutilizar esta mensagem");
  const range = document.createRange();
  range.setStart(message.firstChild!, 11); range.setEnd(message.firstChild!, 24);
  window.getSelection()?.addRange(range);
  fireEvent.contextMenu(message);
  await user.click(await screen.findByRole("menuitem", { name: "Copiar" }));
  expect(writeClipboardText).toHaveBeenCalledWith("esta mensagem");
  expect(readClipboardText).not.toHaveBeenCalled();
});

it.each(["input", "textarea"] as const)("pastes at the saved %s selection and updates controlled form state in portals", async kind => {
  const user = userEvent.setup();
  function Form() {
    const [value, setValue] = useState("antes depois");
    const Field = kind === "input" ? Input : Textarea;
    return <TextContextMenu><div>{createPortal(<Field aria-label="Nome" value={value} onChange={event => setValue(event.target.value)} />, document.body)}<output>{value}</output></div></TextContextMenu>;
  }
  render(<Form />);
  const input = screen.getByRole<HTMLInputElement | HTMLTextAreaElement>("textbox", { name: "Nome" });
  input.focus(); input.setSelectionRange(6, 12);
  fireEvent.contextMenu(input);
  expect(readClipboardText).not.toHaveBeenCalled();
  await user.click(await screen.findByRole("menuitem", { name: "Colar" }));
  await waitFor(() => expect(input).toHaveValue("antes novo texto"));
  expect(screen.getByRole("status")).toHaveTextContent("antes novo texto");
  expect(input.selectionStart).toBe(16);
});

it("allows copying read-only fields but never offers paste for them", async () => {
  render(<TextContextMenu><div><Input aria-label="Caminho" value="projeto" readOnly /><Input aria-label="Desativado" disabled /></div></TextContextMenu>);
  const input = screen.getByRole<HTMLInputElement>("textbox", { name: "Caminho" });
  input.focus(); input.setSelectionRange(0, 7);
  fireEvent.contextMenu(input);
  expect(await screen.findByRole("menuitem", { name: "Copiar" })).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: "Colar" })).not.toBeInTheDocument();
});

it("pastes plain text through the composer editor and retains its undo history", async () => {
  const user = userEvent.setup();
  const changes = vi.fn();
  function Composer() {
    const [draft, setDraft] = useState<ChatDraft>({ content: "Olá " });
    return <TextContextMenu><div><SkillInput draft={draft} onChange={next => { changes(next); setDraft(next); }} onSend={vi.fn()} disabled={false} compacting={false} working={false}>{null}</SkillInput></div></TextContextMenu>;
  }
  render(<Composer />);
  const input = screen.getByRole("textbox", { name: "Mensagem" });
  await user.click(input);
  await act(async () => {
    const range = document.createRange(); range.selectNodeContents(input); range.collapse(false);
    window.getSelection()?.removeAllRanges(); window.getSelection()?.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
  });
  fireEvent.contextMenu(input);
  await user.click(await screen.findByRole("menuitem", { name: "Colar" }));
  await waitFor(() => expect(input).toHaveTextContent("Olá novo texto"));
  expect(changes).toHaveBeenLastCalledWith(expect.objectContaining({ content: "Olá novo texto" }));
  await act(async () => { input.focus(); });
  await user.keyboard("{Control>}z{/Control}");
  await waitFor(() => expect(input).toHaveTextContent("Olá"));
  expect(input).not.toHaveTextContent("novo texto");
});
