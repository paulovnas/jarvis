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
      <div className="flex max-w-xl flex-col items-end gap-2">
        {message.attachments && message.attachments.length > 0 && (
          <div className="flex flex-wrap justify-end gap-1.5">
            {message.attachments.map((att) => (
              <div
                key={att.id}
                className="flex items-center gap-2 rounded-lg border border-[#3e4451] bg-[#21252b] px-3.5 py-2 text-xs text-[#abb2bf] shadow-sm"
              >
                <FileCode2 className="size-3.5 text-[#61afef]" />
                <span className="font-mono text-[11px] text-[#e6e6e6]">
                  {att.name}
                </span>
                {att.size && (
                  <span className="text-[10px] text-[#7f848e]">({att.size})</span>
                )}
              </div>
            ))}
          </div>
        )}

        <div className="group relative rounded-2xl rounded-tr-xs border border-[#3e4451]/80 bg-[#2c313a] px-5 py-3.5 text-[14.5px] leading-relaxed text-[#e6e6e6] shadow-md shadow-black/10">
          <p className="whitespace-pre-wrap break-words"><MessageContent content={message.content} parts={message.parts} /></p>
          <div className="mt-2 flex items-center justify-end gap-1.5 text-[11px] text-[#7f848e]">
            <span>{message.timestamp}</span>
            <User className="size-3 text-[#61afef]" />
          </div>
        </div>
      </div>
    </div>
  );
}
