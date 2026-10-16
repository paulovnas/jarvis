import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { EditorContent, Extension, useEditor, useEditorState, type Editor } from "@tiptap/react";
import { Plugin } from "@tiptap/pm/state";
import { markdownEditorExtensions, markdownNeedsSource } from "./markdown-editor-extensions";
import { Bold, Code, CodeXml, Heading2, Italic, List, ListOrdered, Quote, Redo2, Strikethrough, Undo2, WrapText } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Toggle } from "@/components/ui/toggle";
import { Textarea } from "@/components/TextInput";
import { cn } from "@/lib/utils";

interface Props {
  id?: string;
  label: string;
  value: string;
  onChange?: (value: string) => void;
  disabled?: boolean;
  readOnly?: boolean;
  maxLength?: number;
  placeholder?: string;
  spellCheck?: boolean;
  className?: string;
  contentClassName?: string;
}

const formats = [
  { name: "bold", label: "Negrito", Icon: Bold, run: (editor: Editor) => editor.chain().focus().toggleBold().run() },
  { name: "italic", label: "Itálico", Icon: Italic, run: (editor: Editor) => editor.chain().focus().toggleItalic().run() },
  { name: "strike", label: "Tachado", Icon: Strikethrough, run: (editor: Editor) => editor.chain().focus().toggleStrike().run() },
  { name: "heading", label: "Título", Icon: Heading2, run: (editor: Editor) => editor.chain().focus().toggleHeading({ level: 2 }).run() },
  { name: "bulletList", label: "Lista com marcadores", Icon: List, run: (editor: Editor) => editor.chain().focus().toggleBulletList().run() },
  { name: "orderedList", label: "Lista numerada", Icon: ListOrdered, run: (editor: Editor) => editor.chain().focus().toggleOrderedList().run() },
  { name: "blockquote", label: "Citação", Icon: Quote, run: (editor: Editor) => editor.chain().focus().toggleBlockquote().run() },
  { name: "code", label: "Código em linha", Icon: Code, run: (editor: Editor) => editor.chain().focus().toggleCode().run() },
  { name: "codeBlock", label: "Bloco de código", Icon: CodeXml, run: (editor: Editor) => editor.chain().focus().toggleCodeBlock().run() },
];

export function MarkdownEditor({ id, label, value, onChange, disabled = false, readOnly = false, maxLength, placeholder, spellCheck = false, className, contentClassName }: Props) {
  const [mode, setMode] = useState("editor");
  const lastValue = useRef(value);
  const extensions = useMemo(() => [
    ...markdownEditorExtensions(),
    Extension.create({
      name: "markdownLimit",
      addProseMirrorPlugins() {
        const editor = this.editor;
        return [new Plugin({ filterTransaction: (transaction, state) => {
          if (!transaction.docChanged || transaction.getMeta("preventUpdate") || maxLength === undefined) return true;
          const next = editor.markdown?.serialize(transaction.doc.toJSON()).length ?? 0;
          const previous = editor.markdown?.serialize(state.doc.toJSON()).length ?? 0;
          return next <= maxLength || next < previous;
        } })];
      },
    }),
  ], [maxLength]);
  const bodyClass = cn("min-h-48 max-h-[55dvh] overflow-y-auto overscroll-contain px-4 py-3 outline-none", contentClassName);
  const editor = useEditor({
    extensions, content: value, contentType: "markdown", editable: !disabled && !readOnly,
    editorProps: {
      attributes: { id: id ?? "", role: "textbox", "aria-label": label, "aria-multiline": "true", class: cn("markdown-editor-prose", bodyClass), spellcheck: String(spellCheck), autocorrect: spellCheck ? "on" : "off", autocapitalize: spellCheck ? "sentences" : "off", "data-placeholder": placeholder ?? "Escreva em Markdown…" },
      handlePaste: (view, event) => {
        if (event.clipboardData?.getData("text/html") && !editor?.isActive("codeBlock")) return false;
        const text = event.clipboardData?.getData("text/plain");
        if (text === undefined) return false;
        if (editor?.isActive("codeBlock") || markdownNeedsSource(text, editor?.markdown)) view.dispatch(view.state.tr.insertText(text));
        else editor?.commands.insertContent(text, { contentType: "markdown" });
        return true;
      },
    },
    onUpdate: ({ editor }) => {
      const next = editor.isEmpty ? "" : editor.getMarkdown();
      lastValue.current = next;
      onChange?.(next);
    },
  });
  const sourceOnly = markdownNeedsSource(value, editor?.markdown);
  const editable = !disabled && !readOnly && !sourceOnly;
  const state = useEditorState({ editor, selector: ({ editor }) => ({
    active: formats.map(format => editor?.isActive(format.name) ?? false),
    undo: editor?.can().undo() ?? false, redo: editor?.can().redo() ?? false,
  }) });
  useLayoutEffect(() => {
    if (editor && value !== lastValue.current) {
      lastValue.current = value;
      editor.commands.setContent(value, { contentType: "markdown", emitUpdate: false });
    }
  }, [editor, value]);
  useEffect(() => {
    editor?.setEditable(editable, false);
    editor?.view.dom.setAttribute("aria-disabled", String(disabled));
    editor?.view.dom.setAttribute("aria-readonly", String(!editable));
  }, [editor, editable, disabled]);

  return <Tabs value={mode} onValueChange={value => setMode(String(value))} className={cn("min-w-0 gap-0 overflow-hidden rounded-md border border-input bg-background focus-within:border-ring", disabled && "opacity-50", className)}>
    <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border bg-muted/20 px-2 py-1.5">
      <TabsList aria-label={`Modo de edição: ${label}`}>
        <TabsTrigger value="editor" className="cursor-pointer"><WrapText />Editor</TabsTrigger>
        <TabsTrigger value="code" className="cursor-pointer"><Code />Código</TabsTrigger>
      </TabsList>
      {mode === "editor" && !readOnly && !sourceOnly && <div role="group" aria-label={`Formatação: ${label}`} className="flex flex-wrap items-center gap-0.5">
        {formats.map((format, index) => <Hint key={format.name} content={format.label}><Toggle type="button" size="sm" className="cursor-pointer px-1.5" aria-label={format.label} pressed={state?.active[index] ?? false} disabled={!editable || !editor} onMouseDown={event => event.preventDefault()} onPressedChange={() => { if (editor) format.run(editor); }}><format.Icon /></Toggle></Hint>)}
        <Hint content="Desfazer"><Button type="button" variant="ghost" size="icon-sm" aria-label="Desfazer" className="cursor-pointer" disabled={!editable || !state?.undo} onClick={() => editor?.chain().focus().undo().run()}><Undo2 /></Button></Hint>
        <Hint content="Refazer"><Button type="button" variant="ghost" size="icon-sm" aria-label="Refazer" className="cursor-pointer" disabled={!editable || !state?.redo} onClick={() => editor?.chain().focus().redo().run()}><Redo2 /></Button></Hint>
      </div>}
    </div>
    {sourceOnly && !readOnly && mode === "editor" && <Alert className="rounded-none border-0 border-b"><AlertDescription>Este documento contém HTML, metadados ou notas de rodapé. Use Código para editar esses trechos sem perder sua estrutura.</AlertDescription></Alert>}
    <TabsContent value="editor" hidden={mode !== "editor"} keepMounted className="m-0 min-h-0 min-w-0"><EditorContent editor={editor} /></TabsContent>
    <TabsContent value="code" hidden={mode !== "code"} className="m-0 min-h-0 min-w-0"><Textarea id={id ? `${id}-code` : undefined} aria-label={label} value={value} readOnly={readOnly} disabled={disabled} maxLength={maxLength} placeholder={placeholder} onChange={event => onChange?.(event.target.value)} className={cn(bodyClass, "w-full resize-y rounded-none border-0 font-mono text-xs leading-6 shadow-none focus-visible:ring-0")} /></TabsContent>
  </Tabs>;
}
