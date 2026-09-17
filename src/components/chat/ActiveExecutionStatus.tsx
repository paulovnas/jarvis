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
    className="mb-2 flex min-h-11 min-w-0 items-center gap-2.5 rounded-lg border border-border bg-card/95 px-3 py-2 text-muted-foreground shadow-[inset_0_1px_0_#ffffff0d,0_8px_24px_#00000026] backdrop-blur motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-2 motion-safe:duration-200"
  >
    <span aria-hidden="true" className="grid size-7 shrink-0 place-items-center rounded-md border border-border bg-secondary/70 shadow-[inset_0_1px_0_#ffffff0d]">
      {status.retry ? <Wifi className="size-3.5 text-onedark-yellow" /> : <Spinner className="text-primary motion-reduce:animate-none" />}
    </span>
    <Hint content={status.retry?.message ?? status.heading} whenTruncated={!status.retry}>
      <span role="status" aria-live="polite" aria-atomic="true" className={`min-w-0 flex-1 truncate text-sm font-medium ${status.waiting ? "text-onedark-yellow" : "text-foreground"} ${!status.waiting && !status.retry ? "reasoning-shimmer" : ""}`}>
        {status.heading}
      </span>
    </Hint>
    <span aria-hidden="true" className="text-border">·</span>
    <span aria-label="Tempo total da execução" className="shrink-0 font-mono text-[11px] tabular-nums text-muted-foreground">
      {formatExecutionDuration(durationMs)}
    </span>
    {status.tools.length > 0 && <span className="hidden shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground sm:inline">
      · {status.tools.length} {status.tools.length === 1 ? "ação" : "ações"}
    </span>}
  </section>;
}
