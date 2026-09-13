import { AlertCircle, BrainCircuit, Check, ChevronRight, Layers3, Sparkles, TriangleAlert, Wifi } from "lucide-react";
import { useMemo, type ReactNode } from "react";
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
  const tools = useMemo(() => allTools.filter(tool => tool.name !== "ask_user"), [allTools]);
  const timeline = useMemo(() => buildExecutionTimeline(work.steps), [work.steps]);
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
        <ExecutionTimeline items={timeline} detailContext={work.detailContext} isStreaming={isStreaming} />
        {work.steps.length === 0 && !retry && <p className="py-1 text-xs">Conectando ao provedor…</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}

type ReasoningThought = { id: string; content: string };
type TimelineItem =
  | { id: string; kind: "reasoning"; thoughts: ReasoningThought[] }
  | { id: string; kind: "commentary"; content: string }
  | { id: string; kind: "tool-group"; group: ReturnType<typeof groupToolActivity>[number] }
  | { id: string; kind: "tool"; tool: ToolCallItem };

function buildExecutionTimeline(steps: AssistantWorkData["steps"]): TimelineItem[] {
  const items: TimelineItem[] = [];
  let pendingTools: ToolCallItem[] = [];
  const flushTools = () => {
    if (pendingTools.length === 0) return;
    const groups = groupToolActivity(pendingTools);
    if (groups.length > 0) groups.forEach(group => items.push({ id: `group:${group.id}`, kind: "tool-group", group }));
    else pendingTools.forEach(tool => items.push({ id: `tool:${tool.id}`, kind: "tool", tool }));
    pendingTools = [];
  };

  steps.forEach((step, stepIndex) => {
    const thoughts = reasoningSections(step.thinking).map((content, partIndex) => ({ id: `${stepIndex}:thinking:${partIndex}`, content }));
    const commentary = step.commentary.trim();
    if (thoughts.length > 0 || commentary) flushTools();
    if (thoughts.length >= 4) items.push({ id: thoughts[0].id, kind: "reasoning", thoughts });
    else thoughts.forEach(thought => items.push({ id: thought.id, kind: "reasoning", thoughts: [thought] }));
    if (commentary) items.push({ id: `${stepIndex}:commentary`, kind: "commentary", content: commentary });
    pendingTools.push(...step.tools.filter(tool => tool.name !== "ask_user"));
  });
  flushTools();
  return items;
}

function ExecutionTimeline({ items, detailContext, isStreaming }: { items: TimelineItem[]; detailContext?: AssistantWorkData["detailContext"]; isStreaming: boolean }) {
  if (items.length === 0) return null;
  return <div className="min-w-0">
    <div className="flex items-center gap-2 px-1 pb-1">
      <span className="shrink-0 font-mono text-[9px] font-medium uppercase tracking-[0.14em] text-muted-foreground/70">Linha do tempo</span>
      <span aria-hidden="true" className="h-px min-w-4 flex-1 bg-border/60" />
    </div>
    <ol aria-label="Linha do tempo da execução" className="relative flex min-w-0 flex-col gap-0.5 before:absolute before:top-3 before:bottom-3 before:left-2.5 before:w-px before:bg-gradient-to-b before:from-onedark-purple/35 before:via-onedark-cyan/30 before:to-border">
      {items.map((item, index) => {
        const isLatest = index === items.length - 1;
        const active = isStreaming && isLatest;
        return <TimelineRow key={item.id} index={index} kind={item.kind} active={active} animate={isStreaming}>
          {item.kind === "reasoning" ? item.thoughts.length > 1
            ? <ReasoningGroup thoughts={item.thoughts} />
            : <ReasoningItem content={item.thoughts[0].content} />
          : item.kind === "commentary" ? <div className="flex min-w-0 items-start gap-2 rounded-md px-2.5 py-2 text-xs leading-relaxed text-muted-foreground/80 hover:bg-card/25">
            <Sparkles aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-onedark-purple/80" />
            <p className="whitespace-pre-wrap break-words italic">{item.content}</p>
          </div>
          : item.kind === "tool-group" ? <ToolActivityGroup group={item.group} isLatest={isLatest} detailContext={detailContext} />
          : <ToolCallCard tool={item.tool} detailContext={detailContext} />}
        </TimelineRow>;
      })}
    </ol>
  </div>;
}

function TimelineRow({ index, kind, active, animate, children }: { index: number; kind: TimelineItem["kind"]; active: boolean; animate: boolean; children: ReactNode }) {
  const tone = kind === "reasoning" ? "border-onedark-purple/45 text-onedark-purple"
    : kind === "commentary" ? "border-onedark-yellow/40 text-onedark-yellow"
      : kind === "tool-group" ? "border-onedark-cyan/45 text-onedark-cyan"
        : "border-primary/40 text-primary";
  return <li data-timeline-kind={kind} aria-current={active ? "step" : undefined} className={`relative min-w-0 pl-7 ${animate ? "motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-150" : ""}`}>
    <span aria-hidden="true" className={`absolute top-2 left-0 z-10 flex size-5 items-center justify-center rounded-full border bg-background font-mono text-[8px] font-semibold tabular-nums ${tone} ${active ? "ring-2 ring-onedark-cyan/15" : ""}`}>{String(index + 1).padStart(2, "0")}</span>
    {children}
  </li>;
}

function ReasoningGroup({ thoughts }: { thoughts: { id: string; content: string }[] }) {
  return <Collapsible>
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-9 w-full max-w-full cursor-pointer justify-start gap-2 rounded-md px-2.5 text-left text-[11px] hover:bg-card/45">
      <BrainCircuit aria-hidden="true" data-icon="inline-start" className="size-3.5 text-onedark-purple" />
      <span className="shrink-0 font-mono text-[9px] font-semibold uppercase tracking-[0.12em] text-onedark-purple">Raciocínio</span>
      <span className="min-w-0 flex-1 truncate italic text-muted-foreground">{reasoningPreview(thoughts[thoughts.length - 1]?.content ?? "")}</span>
      <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{thoughts.length} etapas</span>
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="ml-4 flex min-w-0 flex-col border-l border-onedark-purple/20 pl-2">
      {thoughts.map(thought => <ReasoningItem key={thought.id} content={thought.content} />)}
    </CollapsibleContent>
  </Collapsible>;
}

function ToolActivityGroup({ group, isLatest, detailContext }: { group: ReturnType<typeof groupToolActivity>[number]; isLatest: boolean; detailContext?: AssistantWorkData["detailContext"] }) {
  return <Collapsible key={isLatest ? "latest" : "completed"} defaultOpen={isLatest && group.active} className="min-w-0">
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group flex h-auto min-h-9 w-full cursor-pointer justify-start gap-2 rounded-md bg-card/20 px-2.5 text-left text-[11px] hover:bg-card/50">
      <Layers3 aria-hidden="true" data-icon="inline-start" className="size-3.5 shrink-0 text-onedark-cyan" />
      <Hint content={group.summary}><span className="min-w-0 flex-1 truncate">{group.summary}</span></Hint>
      <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{group.tools.length} {group.tools.length === 1 ? "ação" : "ações"}</span>
      {group.active ? <Spinner aria-label="Grupo em execução" className="motion-reduce:animate-none" /> : group.failures > 0 ? <AlertCircle aria-label={`${group.failures} ${group.failures === 1 ? "falha" : "falhas"}`} className="size-3.5 text-destructive" /> : group.warnings > 0 ? <TriangleAlert aria-label={`${group.warnings} ${group.warnings === 1 ? "aviso" : "avisos"}`} className="size-3.5 text-onedark-yellow" /> : <Check aria-label="Grupo concluído" className="size-3.5 text-onedark-green" />}
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="mt-1 ml-3 flex min-w-0 flex-col gap-0.5 border-l border-onedark-cyan/20 pl-1.5">
      {group.tools.map((tool: ToolCallItem) => <ToolCallCard key={tool.id} tool={tool} detailContext={detailContext} />)}
    </CollapsibleContent>
  </Collapsible>;
}

function ReasoningItem({ content }: { content: string }) {
  return <Collapsible>
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 w-full max-w-full cursor-pointer justify-start gap-2 rounded-md px-2.5 text-left hover:bg-card/35">
      <BrainCircuit aria-hidden="true" data-icon="inline-start" className="text-onedark-purple" />
      <span className="shrink-0 font-mono text-[9px] font-semibold uppercase tracking-[0.12em] text-onedark-purple">Raciocínio</span>
      <span className="min-w-0 truncate text-[11px] italic text-muted-foreground">{reasoningPreview(content) || "Em andamento"}</span>
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="py-2 pl-6 text-sm leading-relaxed"><p className="whitespace-pre-wrap break-words">{content}</p></CollapsibleContent>
  </Collapsible>;
}
