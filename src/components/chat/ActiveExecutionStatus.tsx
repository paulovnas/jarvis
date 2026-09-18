import { Wifi } from "lucide-react";
import type { AgentTurn } from "@/core/chat";
import { Hint } from "@/components/ui/hint";
import { Spinner } from "@/components/ui/spinner";
import { executionDuration, formatExecutionDuration, useRunningClock } from "@/hooks/use-running-clock";
import { describeExecution } from "./execution-status";

export function ActiveExecutionStatus({ turn }: { turn: AgentTurn }) {
  const now = useRunningClock(true);
  const durationMs = executionDuration(turn.createdAt, turn.durationMs, true, now);
  const status = describeExecution({
    durationSeconds: Math.floor(durationMs / 1_000),
    retry: turn.steps[turn.steps.length - 1]?.retry,
    steps: turn.steps.map(step => ({ thinking: step.summary, commentary: step.text, tools: step.tools })),
  }, true);

  return <section
    aria-label="Execução em andamento"
    data-testid="active-execution-status"
    className="mb-1 flex min-h-8 min-w-0 items-center gap-2 px-3 py-1.5 text-muted-foreground motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1 motion-safe:duration-200"
  >
    <span aria-hidden="true" className="relative grid size-5 shrink-0 place-items-center">
      <span className={`absolute inset-1 rounded-full opacity-50 blur-[3px] ${status.retry || status.waiting ? "bg-onedark-yellow" : "bg-primary"}`} />
      {status.retry ? <Wifi className="relative size-3.5 text-onedark-yellow" /> : <Spinner className={`relative size-3.5 motion-reduce:animate-none ${status.waiting ? "text-onedark-yellow" : "text-primary"}`} />}
    </span>
    <Hint content={status.retry?.message ?? status.heading} whenTruncated={!status.retry}>
      <span role="status" aria-live="polite" aria-atomic="true" className={`min-w-0 flex-1 truncate text-[13px] font-medium ${status.waiting ? "text-onedark-yellow" : "text-foreground/90"} ${!status.waiting && !status.retry ? "reasoning-shimmer" : ""}`}>
        {status.heading}
      </span>
    </Hint>
    <span aria-hidden="true" className="hidden h-3 w-px shrink-0 bg-border/70 sm:block" />
    <span className="flex shrink-0 items-center gap-1.5 font-mono text-[10px] tabular-nums text-muted-foreground">
      <span aria-label="Tempo total da execução">{formatExecutionDuration(durationMs)}</span>
      {status.tools.length > 0 && <span className="hidden items-center gap-1.5 sm:inline-flex"><span aria-hidden="true" className="text-border">·</span><span>{status.tools.length} {status.tools.length === 1 ? "ação" : "ações"}</span></span>}
    </span>
  </section>;
}
