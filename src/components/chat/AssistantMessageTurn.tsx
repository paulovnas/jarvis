import { Bot } from "lucide-react";
import { lazy, Suspense } from "react";
import { Badge } from "@/components/ui/badge";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import { QuestionHistory } from "./QuestionHistory";
import type { ChatMessage } from "./types";

const ChatMarkdown = lazy(() => import("./ChatMarkdown"));

export function AssistantMessageTurn({ message }: { message: ChatMessage }) {
  return (
    <article data-testid={`assistant-message-${message.id}`} className="my-5 flex min-w-0 flex-col gap-2">
      <header className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <Bot className="size-5 shrink-0 text-primary" />
          <span className="text-sm font-semibold">Jarvis</span>
          {message.model && <Badge variant="outline" className="max-w-64 truncate text-[10px]">{message.model}</Badge>}
        </div>
        <time className="shrink-0 text-[11px] text-muted-foreground">{message.timestamp}</time>
      </header>
      {message.work && <AssistantWorkCollapse work={message.work} isStreaming={message.streaming} />}
      {message.work?.steps.flatMap(step => step.tools).filter(tool => tool.name === "ask_user").map(tool => <QuestionHistory key={tool.id} tool={tool} />)}
      {message.error && <div role="alert" className="rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm"><p className="font-medium">{message.error.title}</p><p className="mt-1 text-muted-foreground">{message.error.message}</p></div>}
      {message.content && <div className="min-w-0 rounded-2xl border border-border bg-card p-5 text-sm leading-relaxed [&_pre]:my-3 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-background [&_pre]:p-3 [&_code]:font-mono [&_p]:my-3 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5 [&_h1]:text-lg [&_h2]:text-base [&_h3]:font-semibold [&_blockquote]:border-l-2 [&_blockquote]:pl-3 [&_table]:block [&_table]:overflow-x-auto [&_td]:border [&_td]:p-2 [&_th]:border [&_th]:p-2">
        <Suspense fallback={<p className="whitespace-pre-wrap">{message.content}</p>}><ChatMarkdown content={message.content} /></Suspense>
      </div>}
      {message.streaming && !message.content && !message.work && <p role="status" className="text-sm text-muted-foreground">Aguardando o provedor…</p>}
    </article>
  );
}
