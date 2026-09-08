import { JarvisLogo } from "@/components/JarvisLogo";
import { lazy, Suspense } from "react";
import { Badge } from "@/components/ui/badge";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import { QuestionHistory } from "./QuestionHistory";
import { GeneratedImageCard } from "./GeneratedImageCard";
import { BrowserCaptureCard } from "./BrowserCaptureCard";
import type { ChatMessage } from "./types";

const ChatMarkdown = lazy(() => import("./ChatMarkdown"));

export function AssistantMessageTurn({ message }: { message: ChatMessage }) {
  return (
    <article data-testid={`assistant-message-${message.id}`} className="my-5 flex min-w-0 flex-col gap-2">
      <header className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
        <span role="img" aria-label="Jarvis" className="inline-flex shrink-0 items-center gap-1.5">
            <span aria-hidden="true" className="relative h-5 w-3.5 overflow-hidden"><JarvisLogo className="absolute -left-[7px] -top-1 size-7 max-w-none" /></span>
            <span aria-hidden="true" className="text-xs font-semibold tracking-wide text-foreground">arvis</span>
          </span>
          {message.model && <Badge variant="outline" title={message.model} className="max-w-56 truncate border-0 bg-transparent px-1 font-mono text-[10px] text-muted-foreground">{message.model.split(" / ").pop()}</Badge>}
        </div>
        <time className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{message.timestamp}</time>
      </header>
      {message.work && <AssistantWorkCollapse work={message.work} isStreaming={message.streaming} />}
      {message.work?.steps.flatMap(step => step.tools).filter(tool => tool.name === "ask_user").map(tool => <QuestionHistory key={tool.id} tool={tool} />)}
      {message.work?.steps.flatMap(step => step.tools).filter(tool => tool.name === "generate_image").map(tool => <GeneratedImageCard key={tool.id} tool={tool} />)}
      {message.work?.steps.flatMap(step => step.tools).filter(tool => tool.name === "browser_screenshot").map(tool => <BrowserCaptureCard key={tool.id} tool={tool} />)}
      {message.error && <div role="alert" className="rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm"><p className="font-medium">{message.error.title}</p><p className="mt-1 text-muted-foreground">{message.error.message}</p></div>}
      {message.content && <div className="assistant-prose min-w-0 py-2 text-sm leading-7 [&_pre]:my-3 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-border [&_pre]:bg-card [&_pre]:p-3 [&_pre]:text-xs [&_code]:font-mono [&_p]:my-3 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5 [&_h1]:text-lg [&_h2]:text-base [&_h3]:font-semibold [&_blockquote]:border-l-2 [&_blockquote]:pl-3 [&_table]:block [&_table]:overflow-x-auto [&_td]:border [&_td]:p-2 [&_th]:border [&_th]:p-2">
        <Suspense fallback={<p className="whitespace-pre-wrap">{message.content}</p>}><ChatMarkdown content={message.content} /></Suspense>
      </div>}
      {message.streaming && !message.content && !message.work && <p role="status" className="text-sm text-muted-foreground">Aguardando o provedor…</p>}
    </article>
  );
}
