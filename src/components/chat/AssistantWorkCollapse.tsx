import { AlertCircle, BrainCircuit, Check, ChevronRight, Layers3, MessageSquare, Wifi } from "lucide-react";
import { useMemo } from "react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import { formatExecutionDuration } from "@/hooks/use-running-clock";
import { ToolCallCard } from "./ToolCallCard";
import { groupToolActivity } from "./tool-activity";
import type { AssistantWorkData, ToolCallItem } from "./types";
import { reasoningPreview, reasoningSections } from "./reasoning-preview";

export function AssistantWorkCollapse({ work, isStreaming = false }: { work: AssistantWorkData; isStreaming?: boolean }) {
  const allTools = useMemo(() => work.steps.flatMap(step => step.tools), [work.steps]);
  const tools = useMemo(() => allTools.filter(tool => tool.name !== "ask_user"), [allTools]);
  const groups = useMemo(() => groupToolActivity(tools), [tools]);
  const thoughts = useMemo(() => work.steps.flatMap((step, index) => [
    ...reasoningSections(step.thinking).map((content, part) => ({ id: `${index}:thinking:${part}`, content, commentary: false })),
    ...(step.commentary ? [{ id: `${index}:commentary`, content: step.commentary, commentary: true }] : []),
  ]), [work.steps]);
  const waiting = allTools.some(tool => tool.name === "ask_user" && (tool.status === "running" || tool.status === "pending"));
  const failures = tools.filter(tool => tool.status === "error").length;
  const latestThinking = [...work.steps].reverse().find(step => step.thinking.trim())?.thinking ?? "";
  const preview = reasoningPreview(latestThinking);
  const retry = isStreaming ? work.retry : undefined;
  const heading = retry ? `Reconectando ${retry.attempt}/${retry.maxAttempts}` : isStreaming ? waiting ? "Aguardando sua resposta" : preview || "Trabalhando…" : `Trabalhou por ${formatExecutionDuration(work.durationSeconds * 1_000)}`;

  return (
    <Collapsible key={isStreaming ? "running" : "finished"} defaultOpen={isStreaming} render={<section aria-label="Processamento do Jarvis" />} className="min-w-0 text-muted-foreground">
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 max-w-full cursor-pointer justify-start gap-2 px-1 text-left text-[11px]">
        {retry ? <Wifi aria-hidden="true" className="text-onedark-yellow" data-icon="inline-start" /> : isStreaming ? <Spinner aria-label="Em execução" className="motion-reduce:animate-none" data-icon="inline-start" /> : <BrainCircuit aria-hidden="true" data-icon="inline-start" />}
        <span role={retry ? "status" : undefined} className={`min-w-0 truncate ${isStreaming && !waiting ? "reasoning-shimmer" : ""}`} title={heading}>{heading}</span>
        {isStreaming && <span aria-label="Tempo total da execução" className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">· {formatExecutionDuration(work.durationSeconds * 1_000)}</span>}
        {tools.length > 0 && <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">· {tools.length} {tools.length === 1 ? "ação" : "ações"}</span>}
        {failures > 0 && <span className="sr-only">{failures} {failures === 1 ? "ação não concluída" : "ações não concluídas"}</span>}
        <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      </CollapsibleTrigger>
      <CollapsibleContent className="flex min-w-0 flex-col gap-1.5 py-2">
        {retry && <p className="rounded-md border border-onedark-yellow/20 bg-onedark-yellow/5 px-3 py-2 text-xs text-onedark-yellow">{retry.message}</p>}
        {thoughts.length >= 4 ? <ReasoningGroup thoughts={thoughts} /> : thoughts.map(thought => <ReasoningItem key={thought.id} content={thought.content} commentary={thought.commentary} />)}
        {groups.length > 0 ? groups.map(group => <ToolActivityGroup key={group.id} group={group} detailContext={work.detailContext} />) : tools.map(tool => <ToolCallCard key={tool.id} tool={tool} detailContext={work.detailContext} />)}
        {work.steps.length === 0 && !retry && <p className="py-1 text-xs">Conectando ao provedor…</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}

function ReasoningGroup({ thoughts }: { thoughts: { id: string; content: string; commentary: boolean }[] }) {
  return <Collapsible>
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-9 max-w-full cursor-pointer justify-start gap-2 px-2.5 text-left text-[11px]">
      <BrainCircuit aria-hidden="true" data-icon="inline-start" className="size-3.5 text-onedark-purple" />
      <span className="min-w-0 flex-1 truncate">Raciocínio e observações</span>
      <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{thoughts.length}</span>
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="ml-4 flex min-w-0 flex-col border-l border-border/70 pl-2">
      {thoughts.map(thought => <ReasoningItem key={thought.id} content={thought.content} commentary={thought.commentary} />)}
    </CollapsibleContent>
  </Collapsible>;
}

function ToolActivityGroup({ group, detailContext }: { group: ReturnType<typeof groupToolActivity>[number]; detailContext?: AssistantWorkData["detailContext"] }) {
  return <Collapsible defaultOpen={group.active} className="rounded-md border border-border/60 bg-card/35">
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group flex h-auto min-h-9 w-full cursor-pointer justify-start gap-2 px-2.5 text-left text-[11px]">
      <Layers3 aria-hidden="true" data-icon="inline-start" className="size-3.5 shrink-0 text-onedark-cyan" />
      <span className="min-w-0 flex-1 truncate" title={group.summary}>{group.summary}</span>
      <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{group.tools.length} {group.tools.length === 1 ? "ação" : "ações"}</span>
      {group.active ? <Spinner aria-label="Grupo em execução" className="motion-reduce:animate-none" /> : group.failures > 0 ? <AlertCircle aria-label={`${group.failures} ${group.failures === 1 ? "falha" : "falhas"}`} className="size-3.5 text-destructive" /> : <Check aria-label="Grupo concluído" className="size-3.5 text-onedark-green" />}
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="mx-2 mb-2 flex min-w-0 flex-col border-l border-border/70 pl-1.5">
      {group.tools.map((tool: ToolCallItem) => <ToolCallCard key={tool.id} tool={tool} detailContext={detailContext} />)}
    </CollapsibleContent>
  </Collapsible>;
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
