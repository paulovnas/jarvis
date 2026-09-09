import { Check, CircleAlert, Cpu, Database, Layers3, Radio, Sparkles } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { BOOTSTRAP_STEPS, bootstrapPercent, type BootstrapProgressState, type BootstrapStepId } from "@/core/bootstrap-state";
import { cn } from "@/lib/utils";

const STEP_ICONS: Record<BootstrapStepId, typeof Cpu> = {
  configuration: Database,
  core: Cpu,
  providers: Radio,
  skills: Sparkles,
  workspace: Layers3,
};

export function BootstrapScreen({ steps }: { steps: BootstrapProgressState }) {
  const percent = bootstrapPercent(steps);
  const active = [...BOOTSTRAP_STEPS]
    .reverse()
    .find((step) => steps[step.id].status === "running") ?? BOOTSTRAP_STEPS.find((step) => steps[step.id].status === "pending");
  const detail = active ? steps[active.id].detail : "Ambiente pronto";

  return <main className="bootstrap-screen dark relative flex min-h-0 flex-1 items-center justify-center overflow-hidden bg-background p-5 sm:p-8">
    <div aria-hidden="true" className="bootstrap-ambient pointer-events-none absolute inset-0" />
    <Card className="instrument-panel relative z-10 w-full max-w-2xl overflow-hidden border-border/80 bg-card/95 p-0 shadow-2xl shadow-black/35 backdrop-blur-xl">
      <CardContent className="p-0">
        <div className="relative overflow-hidden border-b border-border bg-sidebar/75 px-6 py-7 sm:px-8">
          <div aria-hidden="true" className="absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-onedark-cyan/70 to-transparent" />
          <div className="flex items-center justify-between gap-5">
            <div className="min-w-0 space-y-2">
              <p className="micro-label text-onedark-cyan">Inicialização do sistema</p>
              <h1 className="text-lg font-medium tracking-tight text-foreground">Preparando seu ambiente</h1>
              <p aria-live="polite" className="truncate text-xs text-muted-foreground">{detail}</p>
            </div>
            <JarvisLogo variant="icon" className="bootstrap-mark size-14 shrink-0 sm:size-16" />
          </div>
        </div>

        <div className="space-y-6 px-6 py-6 sm:px-8">
          <div className="space-y-2.5" role="status" aria-label="Iniciando o Jarvis">
            <div className="flex items-center justify-between gap-4">
              <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">Bootstrap</span>
              <span className="font-mono text-xs tabular-nums text-onedark-cyan">{percent}%</span>
            </div>
            <Progress value={percent} aria-label="Progresso da inicialização" className="bootstrap-progress [&_[data-slot=progress-track]]:h-1.5 [&_[data-slot=progress-indicator]]:bg-onedark-cyan" />
          </div>

          <div className="grid gap-2 sm:grid-cols-2">
            {BOOTSTRAP_STEPS.map((step) => {
              const state = steps[step.id];
              const Icon = STEP_ICONS[step.id];
              const done = state.status === "complete";
              const warning = state.status === "warning";
              return <div
                key={step.id}
                data-status={state.status}
                className={cn(
                  "bootstrap-step flex min-w-0 items-center gap-3 rounded-md border border-border bg-secondary/25 px-3 py-2.5 transition-colors duration-200 motion-reduce:transition-none",
                  state.status === "running" && "border-onedark-cyan/30 bg-onedark-cyan/5",
                  done && "border-onedark-green/20",
                  warning && "border-onedark-yellow/25",
                )}
              >
                <span className={cn(
                  "flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-card text-muted-foreground",
                  state.status === "running" && "bootstrap-step-active border-onedark-cyan/35 text-onedark-cyan",
                  done && "border-onedark-green/30 text-onedark-green",
                  warning && "border-onedark-yellow/30 text-onedark-yellow",
                )}>
                  {done ? <Check className="size-3.5" /> : warning ? <CircleAlert className="size-3.5" /> : <Icon className="size-3.5" />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-xs font-medium">{step.label}</span>
                  <span className="mt-0.5 block truncate font-mono text-[9px] text-muted-foreground">{state.detail}</span>
                </span>
                {state.status === "running" && <Badge variant="outline" className="shrink-0 border-onedark-cyan/25 text-onedark-cyan">{Math.round(state.progress * 100)}%</Badge>}
              </div>;
            })}
          </div>

          <p className="text-center font-mono text-[9px] uppercase tracking-[0.12em] text-muted-foreground/70">Os dados preparados aqui serão reutilizados durante esta sessão.</p>
        </div>
      </CardContent>
    </Card>
  </main>;
}
