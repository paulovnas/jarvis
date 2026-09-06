import { BookOpen } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import type { MessagePart } from "@/core/chat";
import { AttachmentPreview } from "./AttachmentPreview";

export function MessageContent({ content, parts }: { content: string; parts?: MessagePart[] }) {
  return <>{parts?.some(part => part.type === "attachment") && <span className="block">{parts.filter(part => part.type === "attachment").map(part => <AttachmentPreview key={part.attachment.id} attachment={part.attachment} />)}</span>}{parts?.length ? parts.map((part, index) => part.type === "attachment" ? null : part.type === "text" ? <span key={index}>{part.text}</span> : <Badge key={index} variant="outline" title={`Skill: ${part.name}`} className="mx-0.5 inline-flex gap-1 border-[#c678dd]/30 bg-[#c678dd]/10 align-baseline text-[#c678dd]"><BookOpen aria-hidden="true" />{part.name}</Badge>) : content}</>;
}
