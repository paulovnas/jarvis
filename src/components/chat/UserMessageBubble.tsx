import { FileCode2, User } from "lucide-react";
import type { ChatMessage } from "./types";
import { MessageContent } from "./MessageContent";

interface UserMessageBubbleProps {
  message: ChatMessage;
}

export function UserMessageBubble({ message }: UserMessageBubbleProps) {
  return (
    <div
      data-testid={`user-message-${message.id}`}
      className="my-5 flex w-full justify-end"
    >
      <div className="flex min-w-0 max-w-[90%] flex-col items-end gap-2">
        {message.attachments && message.attachments.length > 0 && (
          <div className="flex flex-wrap justify-end gap-1.5">
            {message.attachments.map((att) => (
              <div
                key={att.id}
                className="flex items-center gap-2 rounded-lg border border-border bg-card px-3.5 py-2 text-xs text-foreground shadow-sm"
              >
                <FileCode2 className="size-3.5 text-[#61afef]" />
                <span className="font-mono text-[11px] text-foreground">
                  {att.name}
                </span>
                {att.size && (
                  <span className="text-[10px] text-muted-foreground">({att.size})</span>
                )}
              </div>
            ))}
          </div>
        )}

        <div className="instrument-panel group relative max-w-full border-primary/15 bg-secondary/60 px-4 py-3 text-sm leading-relaxed text-foreground">
          <p className="whitespace-pre-wrap break-words"><MessageContent content={message.content} parts={message.parts} /></p>
          <div className="mt-2 flex items-center justify-end gap-1.5 font-mono text-[10px] tabular-nums text-muted-foreground">
            <span>{message.timestamp}</span>
            <User className="size-3 text-[#61afef]" />
          </div>
        </div>
      </div>
    </div>
  );
}
