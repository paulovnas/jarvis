import { LazyChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import { useStreamedText } from "@/components/chat/use-streamed-text";

export function CompanionAnswer({ content, global, isStreaming, streamId }: { content: string; global: boolean; isStreaming: boolean; streamId: string }) {
  const displayed = useStreamedText(content, global && isStreaming, streamId);
  return global ? <p className="whitespace-pre-wrap break-words">{displayed}</p> : <LazyChatMarkdown content={content} isStreaming={isStreaming} streamId={streamId} />;
}

export function CompanionReasoning({ content, isStreaming, streamId }: { content: string; isStreaming: boolean; streamId: string }) {
  const displayed = useStreamedText(content, isStreaming, streamId);
  return <>{displayed}</>;
}
