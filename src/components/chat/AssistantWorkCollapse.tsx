import { BrainCircuit, ChevronRight, MessageSquare, Wifi } from "lucide-react";
import { Fragment } from "react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import { ToolCallCard } from "./ToolCallCard";
import type { AssistantWorkData } from "./types";
import { reasoningPreview, reasoningSections } from "./reasoning-preview";

export function AssistantWorkCollapse({ work, isStreaming = false }: { work: AssistantWorkData; isStreaming?: boolean }) {
  const allTools = work.steps.flatMap(step => step.tools);
  const tools = allTools.filter(tool => tool.name !== "ask_user");
  const waiting = allTools.some(tool => tool.name === "ask_user" && (tool.status === "running" || tool.status === "pending"));
  const failures = tools.filter(tool => tool.status === "error").length;
  const latestThinking = [...work.steps].reverse().find(step => step.thinking.trim())?.thinking ?? "";
  const preview = reasoningPreview(latestThinking);
  const retry = isStreaming ? work.retry : undefined;
  const heading = retry ? `Reconectando ${retry.attempt}/${retry.maxAttempts}` : isStreaming ? waiting ? "Aguardando sua resposta" : preview || "Pensando e executando…" : `Trabalhou por ${work.durationSeconds}s`;
  return (
    <Collapsible render={<section aria-label="Processamento do Jarvis" />} className="min-w-0 text-muted-foreground">
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 max-w-full cursor-pointer justify-start gap-2 px-1 text-left text-[11px]">
        {retry ? <Wifi aria-hidden="true" className="text-onedark-yellow" data-icon="inline-start" /> : isStreaming ? <Spinner aria-label="Em execução" className="motion-reduce:animate-none" data-icon="inline-start" /> : <BrainCircuit aria-hidden="true" data-icon="inline-start" />}
        <span role={retry ? "status" : undefined} className={`min-w-0 truncate ${isStreaming && !waiting ? "reasoning-shimmer" : ""}`} title={heading}>{heading}</span>
        {tools.length > 0 && <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">· {tools.length} {tools.length === 1 ? "ação" : "ações"}</span>}
        {failures > 0 && <span className="sr-only">{failures} {failures === 1 ? "ação não concluída" : "ações não concluídas"}</span>}
        <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      </CollapsibleTrigger>
      <CollapsibleContent className="flex min-w-0 flex-col gap-1.5 py-2">
        {retry && <p className="rounded-md border border-onedark-yellow/20 bg-onedark-yellow/5 px-3 py-2 text-xs text-onedark-yellow">{retry.message}</p>}
        {work.steps.map((step, index) => <Fragment key={index}>
          {reasoningSections(step.thinking).map((thinking, part) => <ReasoningItem key={`thinking-${part}`} content={thinking} />)}
          {step.commentary && <ReasoningItem content={step.commentary} commentary />}
          {step.tools.filter(tool => tool.name !== "ask_user").map(tool => <ToolCallCard key={tool.id} tool={tool} />)}
        </Fragment>)}
        {work.steps.length === 0 && !retry && <p className="py-1 text-xs">Conectando ao provedor…</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}

function ReasoningItem({ content, commentary = false }: { content: string; commentary?: boolean }) {
  return <Collapsible>
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 max-w-full cursor-pointer justify-start gap-2 px-1 text-left">
      {commentary ? <MessageSquare aria-hidden="true" data-icon="inline-start" /> : <BrainCircuit aria-hidden="true" data-icon="inline-start" />}
      <span className={`truncate text-[11px] ${commentary ? "" : "italic"}`}>{commentary ? "Observações" : reasoningPreview(content) || "Raciocínio"}</span>
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="py-2 pl-6 text-sm leading-relaxed"><p className={`whitespace-pre-wrap break-words ${commentary ? "text-foreground" : ""}`}>{content}</p></CollapsibleContent>
  </Collapsible>;
}
