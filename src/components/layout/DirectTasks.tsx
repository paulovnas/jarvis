import { Ban, Check, Circle, CircleDot } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import type { DirectTask } from "@/core/chat";
import { ROLE_COLORS } from "@/core/workflow";
import type { CSSProperties } from "react";

const presentation = {
  pending: {
    label: "Pendente",
    icon: Circle,
    className: "border-border bg-card/50 text-muted-foreground",
    iconClassName: "text-muted-foreground/70",
    labelClassName: "text-muted-foreground/70",
  },
  in_progress: {
    label: "Em andamento",
    icon: CircleDot,
    className: "text-foreground",
    iconClassName: "animate-pulse motion-reduce:animate-none",
    labelClassName: "",
  },
  completed: {
    label: "Concluída",
    icon: Check,
    className: "border-onedark-green/20 bg-onedark-green/5 text-muted-foreground",
    iconClassName: "text-onedark-green",
    labelClassName: "text-onedark-green",
  },
  blocked: {
    label: "Bloqueada",
    icon: Ban,
    className: "border-destructive/35 bg-destructive/5 text-foreground",
    iconClassName: "text-destructive",
    labelClassName: "text-destructive",
  },
} as const;

export function DirectTasks({ tasks, active, flow }: { tasks: DirectTask[]; active: boolean; flow: "standard" | "designer" | "custom" }) {
  if (tasks.length === 0) {
    return <p role="status" className="text-xs leading-5 text-muted-foreground">
      {active ? "O agente ainda está organizando o trabalho." : "Nenhuma tarefa registrada nesta solicitação."}
    </p>;
  }
  const completed = tasks.filter(task => task.status === "completed").length;
  const accent = flow === "designer" ? ROLE_COLORS.designer : ROLE_COLORS.builder;
  const accentStyle = { "--task-accent": accent } as CSSProperties;
  return <div className="space-y-3">
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <span className="micro-label text-muted-foreground">Progresso</span>
        <Badge variant="outline" className="font-mono text-[10px] tabular-nums">{completed}/{tasks.length}</Badge>
      </div>
      <Progress aria-label="Progresso das tarefas" value={completed / tasks.length * 100} style={accentStyle} className="[&_[data-slot=progress-indicator]]:bg-[var(--task-accent)]" />
    </div>
    <ol aria-label="Tarefas do agente" className="space-y-1.5">
      {tasks.map(task => {
        const state = presentation[task.status];
        const Icon = state.icon;
        const activeStyle = task.status === "in_progress" ? { borderColor: `color-mix(in srgb, ${accent} 35%, transparent)`, backgroundColor: `color-mix(in srgb, ${accent} 5%, transparent)` } : undefined;
        return <li key={task.id} data-status={task.status} data-working={active && task.status === "in_progress"} style={activeStyle} className={cn("direct-task relative isolate rounded-md border p-2.5 shadow-[inset_0_1px_0_#ffffff0a]", state.className)}>
          <div className="flex min-w-0 items-start gap-2">
            <Icon aria-hidden="true" style={task.status === "in_progress" ? { color: accent } : undefined} className={cn("mt-0.5 size-3.5 shrink-0", state.iconClassName)} />
            <div className="min-w-0 flex-1">
              <p className={cn("break-words text-xs leading-5", task.status === "completed" && "line-through decoration-onedark-green/60")}>{task.title}</p>
              <span style={task.status === "in_progress" ? { color: accent } : undefined} className={cn("mt-1 block font-mono text-[9px] font-semibold uppercase tracking-wider", state.labelClassName)}>{state.label}</span>
            </div>
          </div>
        </li>;
      })}
    </ol>
  </div>;
}
