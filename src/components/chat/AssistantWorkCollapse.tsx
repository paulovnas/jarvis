import { AlertCircle, BrainCircuit, Check, ChevronRight, Layers3, TriangleAlert, Wifi } from "lucide-react";
import { Fragment, useMemo } from "react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Hint } from "@/components/ui/hint";
import { Spinner } from "@/components/ui/spinner";
import { formatExecutionDuration } from "@/hooks/use-running-clock";
import { ToolCallCard } from "./ToolCallCard";
import { groupToolActivity, isTaskReminder } from "./tool-activity";
import type { AssistantWorkData, ToolCallItem } from "./types";
import { reasoningPreview, reasoningSections } from "./reasoning-preview";

export function AssistantWorkCollapse({ work, isStreaming = false }: { work: AssistantWorkData; isStreaming?: boolean }) {
  const allTools = useMemo(() => work.steps.flatMap(step => step.tools), [work.steps]);
  const timeline = useMemo(() => {
    const entries: NarrativeEntry[] = [];
    let toolOffset = 0;
    work.steps.forEach((step, index) => {
      reasoningSections(step.thinking).forEach((content, part) => entries.push({ id: `${index}:thinking:${part}`, kind: "reasoning", content, toolOffset }));
      const commentary = step.commentary.trim();
      if (commentary) entries.push({ id: `${index}:commentary`, kind: "commentary", content: commentary, toolOffset });
      toolOffset += step.tools.filter(tool => tool.name !== "ask_user").length;
    });
    return entries;
  }, [work.steps]);
  const tools = useMemo(() => allTools.filter(tool => tool.name !== "ask_user"), [allTools]);
  const groups = useMemo(() => groupToolActivity(tools), [tools]);
  const latestGroupId = groups[groups.length - 1]?.id;
  const waiting = allTools.some(tool => tool.name === "ask_user" && (tool.status === "running" || tool.status === "pending"));
  const failures = tools.filter(tool => tool.status === "error" && !isTaskReminder(tool)).length;
  const warnings = tools.filter(isTaskReminder).length;
  const latestThinking = [...work.steps].reverse().find(step => step.thinking.trim())?.thinking ?? "";
  const preview = reasoningPreview(latestThinking);
  const retry = isStreaming ? work.retry : undefined;
  const heading = retry ? `Reconectando ${retry.attempt}/${retry.maxAttempts}` : isStreaming ? waiting ? "Aguardando sua resposta" : preview || "Trabalhando…" : `Trabalhou por ${formatExecutionDuration(work.durationSeconds * 1_000)}`;

  return (
    <Collapsible key={isStreaming ? "running" : "finished"} defaultOpen={isStreaming} render={<section aria-label="Processamento do Jarvis" />} className="min-w-0 text-muted-foreground">
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 max-w-full cursor-pointer justify-start gap-2 px-1 text-left text-[11px]">
        {retry ? <Wifi aria-hidden="true" className="text-onedark-yellow" data-icon="inline-start" /> : isStreaming ? <Spinner aria-label="Em execução" className="motion-reduce:animate-none" data-icon="inline-start" /> : <BrainCircuit aria-hidden="true" data-icon="inline-start" />}
        <Hint content={heading}><span role={retry ? "status" : undefined} className={`min-w-0 truncate ${isStreaming && !waiting ? "reasoning-shimmer" : ""}`}>{heading}</span></Hint>
        {isStreaming && <span aria-label="Tempo total da execução" className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">· {formatExecutionDuration(work.durationSeconds * 1_000)}</span>}
        {tools.length > 0 && <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">· {tools.length} {tools.length === 1 ? "ação" : "ações"}</span>}
        {failures > 0 && <span className="sr-only">{failures} {failures === 1 ? "ação não concluída" : "ações não concluídas"}</span>}
        {warnings > 0 && <span className="sr-only">{warnings} {warnings === 1 ? "aviso" : "avisos"}</span>}
        <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      </CollapsibleTrigger>
      <CollapsibleContent className="flex min-w-0 flex-col gap-1.5 py-2">
        {retry && <p className="rounded-md border border-onedark-yellow/20 bg-onedark-yellow/5 px-3 py-2 text-xs text-onedark-yellow">{retry.message}</p>}
        <ExecutionTimeline timeline={timeline} tools={tools} groups={groups} latestGroupId={latestGroupId} detailContext={work.detailContext} />
        {work.steps.length === 0 && !retry && <p className="py-1 text-xs">Conectando ao provedor…</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}

type NarrativeEntry = {
  id: string;
  kind: "reasoning" | "commentary";
  content: string;
  toolOffset: number;
};

type ToolActivityGroups = ReturnType<typeof groupToolActivity>;

function ExecutionTimeline({ timeline, tools, groups, latestGroupId, detailContext }: { timeline: NarrativeEntry[]; tools: ToolCallItem[]; groups: ToolActivityGroups; latestGroupId?: string; detailContext?: AssistantWorkData["detailContext"] }) {
  let offset = 0;
  const batches = groups.length > 0
    ? groups.map(group => {
      const start = offset;
      offset += group.tools.length;
      return { id: group.id, start, end: offset, group, tool: undefined };
    })
    : tools.map(tool => {
      const start = offset;
      offset += 1;
      return { id: tool.id, start, end: offset, group: undefined, tool };
    });

  return <>
    {batches.map(batch => <Fragment key={batch.id}>
      <Narrative entries={timeline.filter(entry => entry.toolOffset >= batch.start && entry.toolOffset < batch.end)} />
      {batch.group
        ? <ToolActivityGroup group={batch.group} isLatest={batch.group.id === latestGroupId} detailContext={detailContext} />
        : batch.tool && <ToolCallCard tool={batch.tool} detailContext={detailContext} />}
    </Fragment>)}
    <Narrative entries={timeline.filter(entry => entry.toolOffset >= tools.length)} />
  </>;
}

function Narrative({ entries }: { entries: NarrativeEntry[] }) {
  const rows: ({ id: string; kind: "reasoning"; thoughts: { id: string; content: string }[] } | { id: string; kind: "commentary"; content: string })[] = [];
  for (const entry of entries) {
    const current = rows[rows.length - 1];
    if (entry.kind === "reasoning" && current?.kind === "reasoning") current.thoughts.push({ id: entry.id, content: entry.content });
    else if (entry.kind === "reasoning") rows.push({ id: entry.id, kind: "reasoning", thoughts: [{ id: entry.id, content: entry.content }] });
    else rows.push({ id: entry.id, kind: "commentary", content: entry.content });
  }
  return <>{rows.map(row => row.kind === "reasoning"
    ? row.thoughts.length >= 4
      ? <ReasoningGroup key={row.id} thoughts={row.thoughts} />
      : row.thoughts.map(thought => <ReasoningItem key={thought.id} content={thought.content} />)
    : <p key={row.id} className="whitespace-pre-wrap break-words px-2 py-1 text-xs italic leading-relaxed text-muted-foreground/80">{row.content}</p>)}</>;
}

function ReasoningGroup({ thoughts }: { thoughts: { id: string; content: string }[] }) {
  return <Collapsible>
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-9 max-w-full cursor-pointer justify-start gap-2 px-2.5 text-left text-[11px]">
      <BrainCircuit aria-hidden="true" data-icon="inline-start" className="size-3.5 text-onedark-purple" />
      <span className="min-w-0 flex-1 truncate">Raciocínio</span>
      <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{thoughts.length}</span>
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="ml-4 flex min-w-0 flex-col border-l border-border/70 pl-2">
      {thoughts.map(thought => <ReasoningItem key={thought.id} content={thought.content} />)}
    </CollapsibleContent>
  </Collapsible>;
}

function ToolActivityGroup({ group, isLatest, detailContext }: { group: ReturnType<typeof groupToolActivity>[number]; isLatest: boolean; detailContext?: AssistantWorkData["detailContext"] }) {
  return <Collapsible key={isLatest ? "latest" : "completed"} defaultOpen={isLatest && group.active} className="rounded-md border border-border/60 bg-card/35">
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group flex h-auto min-h-9 w-full cursor-pointer justify-start gap-2 px-2.5 text-left text-[11px]">
      <Layers3 aria-hidden="true" data-icon="inline-start" className="size-3.5 shrink-0 text-onedark-cyan" />
      <Hint content={group.summary}><span className="min-w-0 flex-1 truncate">{group.summary}</span></Hint>
      <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{group.tools.length} {group.tools.length === 1 ? "ação" : "ações"}</span>
      {group.active ? <Spinner aria-label="Grupo em execução" className="motion-reduce:animate-none" /> : group.failures > 0 ? <AlertCircle aria-label={`${group.failures} ${group.failures === 1 ? "falha" : "falhas"}`} className="size-3.5 text-destructive" /> : group.warnings > 0 ? <TriangleAlert aria-label={`${group.warnings} ${group.warnings === 1 ? "aviso" : "avisos"}`} className="size-3.5 text-onedark-yellow" /> : <Check aria-label="Grupo concluído" className="size-3.5 text-onedark-green" />}
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="mx-2 mb-2 flex min-w-0 flex-col gap-0.5 border-l border-border/60 pl-1.5">
      {group.tools.map((tool: ToolCallItem) => <ToolCallCard key={tool.id} tool={tool} detailContext={detailContext} />)}
    </CollapsibleContent>
  </Collapsible>;
}

function ReasoningItem({ content }: { content: string }) {
  return <Collapsible>
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 max-w-full cursor-pointer justify-start gap-2 px-1 text-left">
      <BrainCircuit aria-hidden="true" data-icon="inline-start" />
      <span className="truncate text-[11px] italic">{reasoningPreview(content) || "Raciocínio"}</span>
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="py-2 pl-6 text-sm leading-relaxed"><p className="whitespace-pre-wrap break-words">{content}</p></CollapsibleContent>
  </Collapsible>;
}
