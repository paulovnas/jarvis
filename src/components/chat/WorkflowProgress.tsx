import { AlertCircle, Check, ChevronRight, Circle } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import type { WorkflowStageResult } from "@/core/workflow";
import { WorkflowOutcome } from "./WorkflowOutcome";

export type WorkflowProgressStage = { id: string; number: string; name: string; activity?: ReactNode; result?: WorkflowStageResult };
type StageState = "running" | "completed" | "error" | "ended" | "blocked" | "waiting" | "queued";
const stateLabels: Record<StageState, string> = { running: "Em execução", completed: "Concluída", error: "Não concluída", ended: "Encerrada", blocked: "Bloqueada", waiting: "Aguardando", queued: "Na fila" };
function stageState(stage: WorkflowProgressStage, fallback: StageState): StageState {
  const status = stage.result?.status;
  return !status ? fallback : status === "failed" ? "error" : status === "cancelled" || status === "interrupted" ? "ended" : status;
}
function indicator(state: StageState) {
  return state === "running" ? <Spinner aria-hidden="true" className="size-3.5 text-primary motion-reduce:animate-none" />
    : state === "error" || state === "blocked" ? <AlertCircle aria-hidden="true" className="size-3.5 text-destructive" />
      : state === "completed" ? <Check aria-hidden="true" className="size-3.5 text-onedark-green" />
        : <Circle aria-hidden="true" className="size-3.5 text-muted-foreground" />;
}

export function WorkflowProgress({ stages, state: fallback }: { stages: WorkflowProgressStage[]; state: StageState }) {
  const current = stages[stages.length - 1];
  const state = stageState(current, fallback);

  return <Collapsible render={<section aria-label="Progresso do fluxo" />} className="min-w-0 rounded-xl border border-border/70 bg-card/40">
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} aria-current={state === "running" ? "step" : undefined} className="group h-auto min-h-12 w-full cursor-pointer justify-start gap-2.5 rounded-xl px-3 py-2 text-left">
      <span className="shrink-0">{indicator(state)}</span>
      <span className="min-w-0 flex-1">
        <span className="block text-[10px] font-normal text-muted-foreground">Fluxo · <span className="font-mono tabular-nums">Etapa {current.number}</span></span>
        <span className="block whitespace-normal break-words text-xs font-medium text-foreground">{current.name}</span>
      </span>
      <span className={`shrink-0 text-[10px] font-normal ${state === "error" || state === "blocked" ? "text-destructive" : "text-muted-foreground"}`}>{stateLabels[state]}</span>
      <ChevronRight aria-hidden="true" className="size-3.5 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      <span className="sr-only">Histórico do fluxo</span>
    </CollapsibleTrigger>
    {current.result && ["blocked", "failed"].includes(current.result.status) && <div className="px-3 pb-3"><WorkflowOutcome result={current.result} compact /></div>}
    <CollapsibleContent className="min-w-0 border-t border-border/70 p-2">
      <ol aria-label="Histórico do fluxo" className="flex min-w-0 flex-col gap-1">
        {stages.map((stage, index) => {
          const latest = index === stages.length - 1;
          const rowState = stageState(stage, latest ? state : "ended");
          return <li key={stage.id} data-execution-phase={stage.id} aria-current={rowState === "running" ? "step" : undefined} className={`min-w-0 rounded-md px-2 py-2 ${rowState === "running" ? "bg-accent/60" : ""}`}>
            <div className="flex min-w-0 items-center gap-2.5">
              <span className="shrink-0">{indicator(rowState)}</span>
              <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">Etapa {stage.number}</span>
              <span className="min-w-0 flex-1 break-words text-xs text-foreground">{stage.name}</span>
              <span className={`shrink-0 text-[10px] ${rowState === "error" || rowState === "blocked" ? "text-destructive" : "text-muted-foreground"}`}>{stateLabels[rowState]}</span>
            </div>
            {stage.result && <WorkflowOutcome result={stage.result} />}
            {stage.activity}
          </li>;
        })}
      </ol>
    </CollapsibleContent>
  </Collapsible>;
}
