import { BookOpen } from "lucide-react";
import { useMemo, type ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import type { MessagePart } from "@/core/chat";
import { AttachmentPreview } from "./AttachmentPreview";
import { LazyChatMarkdown } from "./LazyChatMarkdown";

export function MessageContent({ content, parts, markdown = false }: { content: string; parts?: MessagePart[]; markdown?: boolean }) {
  const projection = useMemo(() => {
    if (!markdown || !parts?.length) return { content };
    let source = "";
    const inlineParts = new Map<string, ReactNode>();
    for (const part of parts) {
      if (part.type === "text") source += part.text;
      else if (part.type === "skill") {
        // Parse one Markdown document so formatting can span an explicit skill chip.
        const token = `JARVISSKILL${crypto.randomUUID().replace(/-/g, "")}TOKEN`;
        source += token;
        inlineParts.set(token, <Badge variant="outline" className="mx-0.5 inline-flex gap-1 border-[#c678dd]/30 bg-[#c678dd]/10 align-baseline text-[#c678dd]"><BookOpen aria-hidden="true" />{part.name}</Badge>);
      }
    }
    return { content: source, inlineParts: inlineParts.size ? inlineParts : undefined };
  }, [content, parts, markdown]);
  return <>{parts?.some(part => part.type === "attachment") && <span className="block">{parts.filter(part => part.type === "attachment").map(part => <AttachmentPreview key={part.attachment.id} attachment={part.attachment} />)}</span>}{markdown ? <LazyChatMarkdown {...projection} /> : parts?.length ? parts.map((part, index) => part.type === "attachment" ? null : part.type === "text" ? <span key={index}>{part.text}</span> : <Badge key={index} variant="outline" className="mx-0.5 inline-flex gap-1 border-[#c678dd]/30 bg-[#c678dd]/10 align-baseline text-[#c678dd]"><BookOpen aria-hidden="true" />{part.name}</Badge>) : content}</>;
}
