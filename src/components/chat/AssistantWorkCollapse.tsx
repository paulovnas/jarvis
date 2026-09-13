import { AlertCircle, BrainCircuit, Check, ChevronRight, Layers3, TriangleAlert, Wifi } from "lucide-react";
import { lazy, Suspense, useMemo } from "react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Hint } from "@/components/ui/hint";
import { Spinner } from "@/components/ui/spinner";
import { formatExecutionDuration } from "@/hooks/use-running-clock";
import { ToolCallCard } from "./ToolCallCard";
import { groupToolActivity, isTaskReminder, summarizeToolActivity } from "./tool-activity";
import type { AssistantWorkData, ToolCallItem } from "./types";
import { reasoningPreview, reasoningSections } from "./reasoning-preview";

const ChatMarkdown = lazy(() => import("./ChatMarkdown"));

export function AssistantWorkCollapse({ work, isStreaming = false }: { work: AssistantWorkData; isStreaming?: boolean }) {
  const allTools = useMemo(() => work.steps.flatMap(step => step.tools), [work.steps]);
  const tools = useMemo(() => allTools.filter(tool => tool.name !== "ask_user"), [allTools]);
  const phases = useMemo(() => buildExecutionPhases(work.steps), [work.steps]);
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
        <ExecutionPhases phases={phases} detailContext={work.detailContext} isStreaming={isStreaming} />
        {work.steps.length === 0 && !retry && <p className="py-1 text-xs">Conectando ao provedor…</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}

type ReasoningThought = { id: string; content: string };
type ActivityItem =
  | { id: string; kind: "reasoning"; thoughts: ReasoningThought[] }
  | { id: string; kind: "tool-group"; group: ReturnType<typeof groupToolActivity>[number] }
  | { id: string; kind: "tool"; tool: ToolCallItem };
type ExecutionPhase = { id: string; observation: string; activity: ActivityItem[] };

function buildExecutionPhases(steps: AssistantWorkData["steps"]): ExecutionPhase[] {
  const phases: ExecutionPhase[] = [];
  let current: ExecutionPhase | undefined;
  let pendingTools: ToolCallItem[] = [];
  const ensurePhase = (id: string) => {
    if (current) return current;
    current = { id, observation: "", activity: [] };
    phases.push(current);
    return current;
  };
  const flushTools = () => {
    if (pendingTools.length === 0) return;
    const phase = ensurePhase("phase:initial");
    const groups = groupToolActivity(pendingTools);
    if (groups.length > 0) groups.forEach(group => phase.activity.push({ id: `group:${group.id}`, kind: "tool-group", group }));
    else pendingTools.forEach(tool => phase.activity.push({ id: `tool:${tool.id}`, kind: "tool", tool }));
    pendingTools = [];
  };

  steps.forEach((step, stepIndex) => {
    const thoughts = reasoningSections(step.thinking).map((content, partIndex) => ({ id: `${stepIndex}:thinking:${partIndex}`, content }));
    const observation = step.commentary.trim();
    if (observation) {
      flushTools();
      current = { id: `${stepIndex}:observation`, observation, activity: [] };
      phases.push(current);
    }
    const phase = ensurePhase("phase:initial");
    if (thoughts.length > 0) flushTools();
    if (thoughts.length >= 4) phase.activity.push({ id: thoughts[0].id, kind: "reasoning", thoughts });
    else thoughts.forEach(thought => phase.activity.push({ id: thought.id, kind: "reasoning", thoughts: [thought] }));
    pendingTools.push(...step.tools.filter(tool => tool.name !== "ask_user"));
  });
  flushTools();
  return phases.filter(phase => phase.observation || phase.activity.length > 0);
}

function ExecutionPhases({ phases, detailContext, isStreaming }: { phases: ExecutionPhase[]; detailContext?: AssistantWorkData["detailContext"]; isStreaming: boolean }) {
  if (phases.length === 0) return null;
  return <div role="list" aria-label="Etapas da execução" className="flex min-w-0 flex-col gap-5">
    {phases.map((phase, phaseIndex) => {
      const current = isStreaming && phaseIndex === phases.length - 1;
      return <section key={phase.id} role="listitem" data-execution-phase={phase.id} aria-current={current ? "step" : undefined} className="min-w-0">
        {phase.observation && <div data-execution-observation className={`min-w-0 px-1 py-2 text-sm leading-7 text-foreground ${isStreaming ? "motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1 motion-safe:duration-200" : ""}`}>
          <div className="assistant-prose min-w-0 break-words [&_code]:font-mono [&_p]:my-0">
            <Suspense fallback={<p className="whitespace-pre-wrap">{phase.observation}</p>}><ChatMarkdown content={phase.observation} /></Suspense>
          </div>
        </div>}
        {phase.activity.length > 0 && <PhaseActivityGroup phase={phase} phaseIndex={phaseIndex} current={current} detailContext={detailContext} isStreaming={isStreaming} />}
      </section>;
    })}
  </div>;
}

function phaseTools(activity: ActivityItem[]): ToolCallItem[] {
  return activity.flatMap(item => item.kind === "tool-group" ? item.group.tools : item.kind === "tool" ? [item.tool] : []);
}

function PhaseActivityGroup({ phase, phaseIndex, current, detailContext, isStreaming }: { phase: ExecutionPhase; phaseIndex: number; current: boolean; detailContext?: AssistantWorkData["detailContext"]; isStreaming: boolean }) {
  const tools = phaseTools(phase.activity);
  const reasoningCount = phase.activity.reduce((total, item) => total + (item.kind === "reasoning" ? item.thoughts.length : 0), 0);
  const actionCount = tools.length + reasoningCount;
  const failures = tools.filter(tool => tool.status === "error" && !isTaskReminder(tool)).length;
  const warnings = tools.filter(isTaskReminder).length;
  const active = (current && isStreaming) || tools.some(tool => tool.status === "running" || tool.status === "pending");
  const summary = tools.length > 0 ? summarizeToolActivity(tools) : reasoningCount > 0 ? "Analisou o contexto" : "Processou a etapa";

  return <Collapsible className="mt-1 min-w-0">
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group flex h-auto min-h-9 w-full cursor-pointer justify-start gap-2 px-1 text-left text-xs text-muted-foreground hover:bg-transparent hover:text-foreground">
      <Layers3 aria-hidden="true" data-icon="inline-start" className="size-3.5 shrink-0 text-onedark-cyan/80" />
      <Hint content={summary}><span className="min-w-0 flex-1 truncate">{summary}</span></Hint>
      <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{actionCount} {actionCount === 1 ? "ação" : "ações"}</span>
      {active ? <Spinner aria-label="Atividades em execução" className="motion-reduce:animate-none" /> : failures > 0 ? <AlertCircle aria-label={`${failures} ${failures === 1 ? "falha" : "falhas"}`} className="size-3.5 text-destructive" /> : warnings > 0 ? <TriangleAlert aria-label={`${warnings} ${warnings === 1 ? "aviso" : "avisos"}`} className="size-3.5 text-onedark-yellow" /> : <Check aria-label="Atividades concluídas" className="size-3.5 text-onedark-green" />}
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="mt-1 ml-2 min-w-0 border-l border-border/60 py-1 pl-3">
      <ol aria-label={`Atividades da etapa ${phaseIndex + 1}`} className="flex min-w-0 flex-col gap-0.5">
        {phase.activity.map((item, itemIndex) => {
          const latest = current && itemIndex === phase.activity.length - 1;
          return <li key={item.id} data-activity-kind={item.kind} aria-current={latest ? "step" : undefined} className={`min-w-0 ${isStreaming ? "motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-150" : ""}`}>
            {item.kind === "reasoning" ? item.thoughts.length > 1
              ? <ReasoningGroup thoughts={item.thoughts} />
              : <ReasoningItem content={item.thoughts[0].content} />
            : item.kind === "tool-group" ? <ToolActivityGroup group={item.group} detailContext={detailContext} />
            : <ToolCallCard tool={item.tool} detailContext={detailContext} />}
          </li>;
        })}
      </ol>
    </CollapsibleContent>
  </Collapsible>;
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

function ToolActivityGroup({ group, detailContext }: { group: ReturnType<typeof groupToolActivity>[number]; detailContext?: AssistantWorkData["detailContext"] }) {
  return <Collapsible className="min-w-0">
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
