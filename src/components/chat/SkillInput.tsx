import { useEffect, useId, useImperativeHandle, useRef, useState, type ReactNode, type Ref } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Node, EditorContent, NodeViewWrapper, ReactNodeViewRenderer, useEditor, type Editor, type JSONContent, type NodeViewProps } from "@tiptap/react";
import Document from "@tiptap/extension-document";
import Paragraph from "@tiptap/extension-paragraph";
import Text from "@tiptap/extension-text";
import HardBreak from "@tiptap/extension-hard-break";
import { UndoRedo } from "@tiptap/extensions";
import { BookOpen, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { InputGroup, InputGroupAddon } from "@/components/ui/input-group";
import { Command, CommandItem, CommandList } from "@/components/ui/command";
import { Skeleton } from "@/components/ui/skeleton";
import { draftText, type ChatDraft, type MessagePart } from "@/core/chat";
import { skillError, skillsSnapshotSchema, type Skill } from "@/core/skills";

function SkillNode({ node, deleteNode, editor }: NodeViewProps) {
  const name = typeof node.attrs.name === "string" ? node.attrs.name : "Skill";
  return <NodeViewWrapper as="span" className="inline" contentEditable={false}>
    <Badge variant="outline" className="mx-0.5 inline-flex gap-1 border-[#c678dd]/30 bg-[#c678dd]/10 align-baseline text-[#c678dd]">
      <BookOpen aria-hidden="true" />{name}
      <Button type="button" variant="ghost" size="icon" className="size-4 cursor-pointer rounded-full p-0 text-current hover:bg-[#c678dd]/20" aria-label={`Remover skill ${name}`} disabled={!editor.isEditable} onClick={() => { if (editor.isEditable) { deleteNode(); editor.view.focus(); } }}><X /></Button>
    </Badge>
  </NodeViewWrapper>;
}
const SkillToken = Node.create({
  name: "skill", group: "inline", inline: true, atom: true, selectable: true,
  addAttributes: () => ({ id: { default: "" }, name: { default: "" } }),
  // Pasted HTML is deliberately not interpreted as an explicit skill invocation.
  parseHTML: () => [],
  renderHTML: ({ node }) => ["span", {}, `/${String(node.attrs.name)}`],
  renderText: ({ node }) => `/${String(node.attrs.name)}`,
  addNodeView: () => ReactNodeViewRenderer(SkillNode),
});

function inputDocument(draft: ChatDraft): JSONContent {
  const parts = draft.parts?.length ? draft.parts : [{ type: "text" as const, text: draft.content }];
  const content: JSONContent[] = [];
  for (const part of parts) {
    if (part.type === "skill") content.push({ type: "skill", attrs: { id: part.id, name: part.name } });
    else part.text.split("\n").forEach((text, index) => {
      if (index) content.push({ type: "hardBreak" });
      if (text) content.push({ type: "text", text });
    });
  }
  return { type: "doc", content: [{ type: "paragraph", content }] };
}

function readDraft(editor: Editor): ChatDraft {
  const parts: MessagePart[] = [];
  const append = (text: string) => {
    const last = parts[parts.length - 1];
    if (last?.type === "text") last.text += text;
    else parts.push({ type: "text", text });
  };
  editor.state.doc.forEach((block, _offset, index) => {
    if (index) append("\n");
    block.forEach(node => {
      if (node.type.name === "skill") parts.push({ type: "skill", id: String(node.attrs.id), name: String(node.attrs.name) });
      else if (node.type.name === "hardBreak") append("\n");
      else if (node.text) append(node.text);
    });
  });
  return { content: draftText(parts), ...(parts.some(part => part.type === "skill") ? { parts } : {}) };
}

interface Query { from: number; to: number; text: string }
function slashQuery(editor: Editor): Query | null {
  const { $from, empty, from } = editor.state.selection;
  if (!empty || !editor.isEditable) return null;
  const before = $from.parent.textBetween(0, $from.parentOffset, "\n", "\ufffc");
  const match = /(?:^|\s)\/([^\s/\ufffc]*)$/.exec(before);
  return match ? { from: from - match[1].length - 1, to: from, text: match[1] } : null;
}
const origins = { jarvis: "Jarvis", agents: ".agents", project: "Projeto" };
const editorProps = {
  attributes: { role: "textbox", "aria-label": "Mensagem", "aria-multiline": "true", "data-placeholder": "Mensagem… / para skills", class: "skill-editor min-h-[84px] max-h-64 w-full overflow-y-auto px-5 pt-4 pb-2 text-[14.5px] leading-relaxed text-foreground outline-none" },
  handlePaste: (view: Editor["view"], event: ClipboardEvent) => {
    const text = event.clipboardData?.getData("text/plain");
    if (text === undefined) return false;
    view.dispatch(view.state.tr.insertText(text));
    return true;
  },
};

interface Props {
  draft: ChatDraft;
  onChange: (draft: ChatDraft) => void;
  onSend: () => void;
  disabled: boolean;
  compacting: boolean;
  working: boolean;
  children: ReactNode;
  ref?: Ref<{ focus: () => void }>;
}

export function SkillInput({ draft, onChange, onSend, disabled, compacting, working, children, ref }: Props) {
  const [query, setQuery] = useState<Query | null>(null);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<{ skills?: Skill[]; error?: string } | null>(null);
  const [selected, setSelected] = useState("");
  const listId = useId();
  const current = useRef(draft);
  // React may commit an older echo after a newer editor transaction. Only external
  // draft replacements (queue restoration / successful send) may reset the document.
  const emittedDrafts = useRef(new WeakSet<ChatDraft>());
  const [initialDocument] = useState(() => inputDocument(draft));
  const refreshQuery = (editor: Editor) => {
    const next = slashQuery(editor);
    setQuery(next);
    if (!next) { setCatalog(null); setDismissed(null); setSelected(""); }
  };
  const editor = useEditor({
    extensions: [Document, Paragraph, Text, HardBreak, UndoRedo, SkillToken],
    content: initialDocument,
    editable: !disabled,
    editorProps,
    onUpdate: ({ editor }) => { current.current = readDraft(editor); emittedDrafts.current.add(current.current); onChange(current.current); refreshQuery(editor); },
    onSelectionUpdate: ({ editor }) => refreshQuery(editor),
    onBlur: () => { setQuery(null); setCatalog(null); },
    onFocus: ({ editor }) => refreshQuery(editor),
  });
  useImperativeHandle(ref, () => ({ focus: () => { if (editor && !editor.isDestroyed) { editor.commands.setTextSelection(editor.state.doc.content.size - 1); editor.view.focus(); } } }), [editor]);
  useEffect(() => {
    if (editor && !editor.isDestroyed && !emittedDrafts.current.has(draft) && JSON.stringify(draft) !== JSON.stringify(current.current)) {
      current.current = draft;
      editor.commands.setContent(inputDocument(draft), { emitUpdate: false });
      editor.commands.setTextSelection(editor.state.doc.content.size - 1);
      setQuery(null);
    }
  }, [draft, editor]);
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    editor.setEditable(!disabled);
    editor.view.dom.setAttribute("aria-disabled", String(disabled));
  }, [editor, disabled]);
  const queryKey = query ? `${query.from}:${query.text}` : null;
  const open = query !== null && dismissed !== queryKey && !disabled;
  useEffect(() => {
    if (!open) return;
    let active = true;
    void invoke("list_skills").then(value => { if (active) setCatalog({ skills: skillsSnapshotSchema.parse(value).skills.filter(skill => skill.enabled) }); })
      .catch(cause => { if (active) setCatalog({ error: skillError(cause) }); });
    return () => { active = false; };
  }, [open]);
  const attached = draft.parts?.filter(part => part.type === "skill").map(part => part.id) ?? [];
  const choices = (catalog?.skills ?? []).filter(skill => !attached.includes(skill.id) && `${skill.name} ${skill.description}`.toLocaleLowerCase().includes(query?.text.toLocaleLowerCase() ?? ""));
  const activeSkill = choices.find(skill => skill.id === selected) ?? choices[0];
  const choose = (skill: Skill) => {
    if (!editor || !query || disabled || attached.length >= 8) return;
    editor.chain().focus().insertContentAt({ from: query.from, to: query.to }, [{ type: "skill", attrs: { id: skill.id, name: skill.name } }, { type: "text", text: " " }]).run();
    setQuery(null); setDismissed(null); setSelected("");
  };
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    editor.view.dom.setAttribute("aria-expanded", String(open));
    if (open) editor.view.dom.setAttribute("aria-controls", listId);
    else editor.view.dom.removeAttribute("aria-controls");
    if (open && activeSkill) editor.view.dom.setAttribute("aria-activedescendant", `${listId}-${activeSkill.id}`);
    else editor.view.dom.removeAttribute("aria-activedescendant");
  }, [editor, open, activeSkill, listId]);
  return <div onKeyDownCapture={event => {
    if (event.nativeEvent.isComposing || editor?.view.composing || disabled) return;
    if (open) {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); setDismissed(queryKey); setCatalog(null); return; }
      if (["ArrowDown", "ArrowUp"].includes(event.key)) {
        event.preventDefault(); event.stopPropagation();
        const index = choices.findIndex(skill => skill.id === activeSkill?.id);
        const next = choices[(index + (event.key === "ArrowDown" ? 1 : -1) + choices.length) % choices.length];
        if (next) setSelected(next.id);
        return;
      }
      if ((event.key === "Enter" && !event.shiftKey) || event.key === "Tab") {
        event.preventDefault(); event.stopPropagation(); if (activeSkill) choose(activeSkill); return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey && event.target === editor?.view.dom) { event.preventDefault(); event.stopPropagation(); onSend(); }
  }}>
    {open && <Command label="Skills disponíveis" shouldFilter={false} value={activeSkill?.id ?? ""} onValueChange={setSelected} className="mx-3 mb-1 h-auto w-auto rounded-xl border border-border bg-card p-1 shadow-lg">
      <CommandList id={listId} aria-label="Skills disponíveis" className="max-h-52">
        {!catalog ? <div role="status" aria-label="Carregando skills" className="flex flex-col gap-3 p-3">{[0, 1, 2].map(i => <div key={i} className="flex gap-3"><Skeleton className="size-5" /><Skeleton className="h-4 w-1/3" /><Skeleton className="h-4 flex-1" /></div>)}</div> : catalog.error ? <p role="alert" className="p-3 text-xs text-destructive">{catalog.error}</p> : attached.length >= 8 ? <p className="p-3 text-xs text-muted-foreground">Limite de 8 skills por mensagem.</p> : choices.length === 0 ? <p className="p-3 text-xs text-muted-foreground">Nenhuma skill encontrada.</p> : choices.map(skill => <CommandItem id={`${listId}-${skill.id}`} key={skill.id} value={skill.id} onSelect={() => choose(skill)} onMouseDown={event => event.preventDefault()} className="cursor-pointer gap-2 py-2 text-xs [&>svg:last-child]:hidden">
          <BookOpen className="text-[#c678dd]" aria-hidden="true" /><span className="max-w-[40%] truncate font-medium">{skill.name}</span><span className="min-w-0 flex-1 truncate text-muted-foreground">{skill.description}</span><span className="shrink-0 text-[10px] text-muted-foreground">{origins[skill.origin]}</span>
        </CommandItem>)}
      </CommandList>
    </Command>}
    <InputGroup aria-label="Mensagem e opções de envio" aria-busy={compacting} data-working={working || undefined} className="chat-composer relative isolate h-auto w-full flex-col items-stretch rounded-[22px] border-border bg-card shadow-2xl shadow-black/30 focus-within:border-primary/60 focus-within:ring-1 focus-within:ring-primary/30 dark:bg-card has-disabled:opacity-100 has-disabled:bg-card dark:has-disabled:bg-card">
      <EditorContent editor={editor} className="min-w-0 w-full" />
      <InputGroupAddon align="block-end" className="p-0 font-normal select-auto">{children}</InputGroupAddon>
    </InputGroup>
  </div>;
}
