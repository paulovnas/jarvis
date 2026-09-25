import { useState, type ReactElement } from "react";
import { Copy, ClipboardPaste } from "lucide-react";
import { toast } from "sonner";
import { readClipboardText, writeClipboardText } from "@/core/clipboard";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger } from "@/components/ui/context-menu";

type TextContext = { text: string; paste?: (text: string) => void };

function dispatchPaste(target: HTMLElement, text: string) {
  const data = new DataTransfer();
  data.setData("text/plain", text);
  return target.dispatchEvent(new ClipboardEvent("paste", { bubbles: true, cancelable: true, clipboardData: data }));
}

function textContext(target: HTMLElement): TextContext {
  const field = target.closest("input, textarea");
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
    const start = field.selectionStart;
    const end = field.selectionEnd;
    const text = start !== null && end !== null ? field.value.slice(start, end) : "";
    const editable = !field.disabled && !field.readOnly && (field instanceof HTMLTextAreaElement || ["text", "search", "url", "tel", "email", "password", "number"].includes(field.type));
    return { text, ...(editable ? { paste: (value: string) => {
      if (!field.isConnected || field.disabled || field.readOnly) return;
      field.focus();
      if (start !== null && end !== null) field.setSelectionRange(start, end);
      if (!dispatchPaste(field, value)) return;
      if (document.execCommand?.("insertText", false, value)) return;
      const from = start ?? 0;
      const to = end ?? field.value.length;
      const limit = field.maxLength < 0 ? value.length : Math.max(0, field.maxLength - field.value.length + to - from);
      const insertion = value.slice(0, limit);
      const next = field.value.slice(0, from) + insertion + field.value.slice(to);
      // Use the native setter so controlled React inputs observe the input event.
      const prototype = field instanceof HTMLInputElement ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
      Object.getOwnPropertyDescriptor(prototype, "value")?.set?.call(field, next);
      if (start !== null) field.setSelectionRange(from + insertion.length, from + insertion.length);
      field.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertFromPaste", data: insertion }));
    } } : {}) };
  }
  const selection = window.getSelection();
  const range = selection?.rangeCount ? selection.getRangeAt(0).cloneRange() : null;
  const editor = target.closest<HTMLElement>('[contenteditable="true"], [contenteditable="plaintext-only"]');
  return { text: selection?.toString() ?? "", ...(editor && editor.getAttribute("aria-disabled") !== "true" ? { paste: (text: string) => {
    if (!editor.isConnected || editor.getAttribute("contenteditable") === "false" || editor.getAttribute("aria-disabled") === "true") return;
    editor.focus();
    const restored = range && editor.contains(range.commonAncestorContainer) ? range : document.createRange();
    if (restored !== range) { restored.selectNodeContents(editor); restored.collapse(false); }
    const current = window.getSelection();
    current?.removeAllRanges();
    current?.addRange(restored);
    // Rich editors (including the composer) own their paste, sanitization and undo.
    if (dispatchPaste(editor, text) && !document.execCommand("insertText", false, text)) {
      throw new Error("Paste unavailable");
    }
  } } : {}) };
}

export function TextContextMenu({ children }: { children: ReactElement }) {
  const [context, setContext] = useState<TextContext>({ text: "" });
  const copy = async () => {
    try { await writeClipboardText(context.text); toast.success("Texto copiado"); }
    catch { toast.error("Não foi possível copiar o texto."); }
  };
  const paste = async () => {
    try { const text = await readClipboardText(); if (text) context.paste?.(text); }
    catch { toast.error("Não foi possível colar o texto."); }
  };
  return <ContextMenu>
    <ContextMenuTrigger render={children} className="select-text" onContextMenu={event => {
      if (event.defaultPrevented || !(event.target instanceof HTMLElement)) { event.preventBaseUIHandler(); return; }
      const next = textContext(event.target);
      if (!next.text && !next.paste) { event.preventDefault(); event.preventBaseUIHandler(); return; }
      setContext(next);
    }} />
    <ContextMenuContent finalFocus={false}>
      {context.text && <ContextMenuItem className="cursor-pointer" onClick={() => void copy()}><Copy />Copiar</ContextMenuItem>}
      {context.paste && <ContextMenuItem className="cursor-pointer" onClick={() => void paste()}><ClipboardPaste />Colar</ContextMenuItem>}
    </ContextMenuContent>
  </ContextMenu>;
}
