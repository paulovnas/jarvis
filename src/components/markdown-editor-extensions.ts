import StarterKit from "@tiptap/starter-kit";
import { Markdown, type MarkdownManager } from "@tiptap/markdown";
import { TableKit } from "@tiptap/extension-table";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import Image from "@tiptap/extension-image";

export function markdownNeedsSource(markdown: string, parser?: MarkdownManager): boolean {
  if (/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/.test(markdown) || /^\[\^[^\]\r\n]+\]:/m.test(markdown)) return true;
  let unsupported = false;
  parser?.instance.walkTokens(parser.instance.lexer(markdown), token => {
    if (token.type === "html" || (token.type === "def" && token.raw.startsWith("[^"))) unsupported = true;
  });
  return unsupported;
}

export function markdownEditorExtensions() {
  return [
    StarterKit.configure({ underline: false, link: { openOnClick: false }, trailingNode: false }),
    Markdown, TableKit, TaskList,
    TaskItem.configure({ nested: true, a11y: { checkboxLabel: node => `Concluir: ${node.textContent || "item sem texto"}` } }),
    Image,
  ];
}
