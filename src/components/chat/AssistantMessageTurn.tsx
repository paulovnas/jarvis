import { JarvisLogo } from "@/components/JarvisLogo";
import { lazy, Suspense, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Download, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { writeClipboardText } from "@/core/clipboard";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import { GeneratedImageCard } from "./GeneratedImageCard";
import { BrowserCaptureCard } from "./BrowserCaptureCard";
import type { ChatMessage } from "./types";

const ChatMarkdown = lazy(() => import("./ChatMarkdown"));

function ResponseActions({ content, fileName }: { content: string; fileName: string }) {
  const [saving, setSaving] = useState(false);
  const copy = async () => {
    try {
      await writeClipboardText(content);
      toast.success("Resposta copiada");
    } catch {
      toast.error("Não foi possível copiar a resposta.");
    }
  };
  const save = async () => {
    if (saving) return;
    setSaving(true);
    try {
      const saved = await invoke<boolean>("save_markdown_document", { content, suggestedFileName: fileName });
      if (saved) toast.success("Resposta salva em Markdown");
    } catch {
      toast.error("Não foi possível salvar a resposta.");
    } finally {
      setSaving(false);
    }
  };
  return <div role="group" aria-label="Ações da resposta" className="flex justify-end gap-0.5 text-muted-foreground">
    <Hint content="Copiar todo o Markdown da resposta">
      <Button type="button" variant="ghost" size="sm" className="cursor-pointer" aria-label="Copiar resposta" onClick={() => { void copy(); }}><Copy data-icon="inline-start" />Copiar</Button>
    </Hint>
    <Hint content="Salvar a resposta como documento .md">
      <Button type="button" variant="ghost" size="sm" className="cursor-pointer" aria-label="Salvar resposta em Markdown" disabled={saving} onClick={() => { void save(); }}><Download data-icon="inline-start" />{saving ? "Salvando…" : "Salvar .md"}</Button>
    </Hint>
  </div>;
}

export function AssistantMessageTurn({ message, onRetry, retrying = false, retryUnavailableReason }: { message: ChatMessage; onRetry?: () => void; retrying?: boolean; retryUnavailableReason?: string }) {
  return (
    <article data-testid={`assistant-message-${message.id}`} className="my-5 flex min-w-0 flex-col gap-2">
      <header className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
        <span role="img" aria-label="Jarvis" className="inline-flex shrink-0 items-center gap-1.5">
            <span aria-hidden="true" className="relative h-5 w-3.5 overflow-hidden"><JarvisLogo className="absolute -left-[7px] -top-1 size-7 max-w-none" /></span>
            <span aria-hidden="true" className="text-xs font-semibold tracking-wide text-foreground">arvis</span>
          </span>
          {message.model && <Hint content={message.model}><Badge variant="outline" className="max-w-56 truncate border-0 bg-transparent px-1 font-mono text-[10px] text-muted-foreground">{message.model.split(" / ").pop()}</Badge></Hint>}
        </div>
        <time className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{message.timestamp}</time>
      </header>
      {message.work && <AssistantWorkCollapse work={message.work} isStreaming={message.streaming} />}
      {message.work?.steps.flatMap(step => step.tools).filter(tool => tool.name === "generate_image").map(tool => <GeneratedImageCard key={tool.id} tool={tool} />)}
      {message.work?.steps.flatMap(step => step.tools).filter(tool => tool.name === "browser_screenshot").map(tool => <BrowserCaptureCard key={tool.id} tool={tool} />)}
      {message.error && <div role="alert" className="rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm">
        <p className="font-medium">{message.error.title}</p>
        <p className="mt-1 text-muted-foreground">{message.error.message}</p>
        {onRetry && <Button type="button" variant="outline" size="sm" className="mt-3 cursor-pointer gap-2 border-destructive/30 bg-background/40" disabled={retrying} onClick={onRetry}>
          <RotateCcw aria-hidden="true" className={retrying ? "animate-spin motion-reduce:animate-none" : ""} />
          {retrying ? "Retomando…" : "Tentar novamente"}
        </Button>}
        {!onRetry && retryUnavailableReason && <p className="mt-3 border-t border-destructive/20 pt-2 text-xs text-muted-foreground">{retryUnavailableReason}</p>}
      </div>}
      {message.content && <div className="assistant-prose min-w-0 py-2 text-sm leading-7 [&_pre]:my-3 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-border [&_pre]:bg-card [&_pre]:p-3 [&_pre]:text-xs [&_code]:font-mono [&_p]:my-3 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5 [&_h1]:text-lg [&_h2]:text-base [&_h3]:font-semibold [&_blockquote]:border-l-2 [&_blockquote]:pl-3 [&_table]:block [&_table]:overflow-x-auto [&_td]:border [&_td]:p-2 [&_th]:border [&_th]:p-2">
        <Suspense fallback={<p className="whitespace-pre-wrap">{message.content}</p>}><ChatMarkdown content={message.content} /></Suspense>
      </div>}
      {message.content && !message.streaming && <ResponseActions content={message.content} fileName={message.exportFileName ?? "Resposta-Jarvis.md"} />}
      {message.streaming && !message.content && !message.work && <p role="status" className="text-sm text-muted-foreground">Aguardando o provedor…</p>}
    </article>
  );
}
