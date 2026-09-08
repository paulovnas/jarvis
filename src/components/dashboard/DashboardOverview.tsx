import { Activity, ArrowDownLeft, ArrowUpRight, CheckCheck, ChevronRight, Cpu, Layers3, MessageSquare, Terminal, Workflow, Zap } from "lucide-react";
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { type Bead, type ProjectMetrics, date, number, statuses } from "@/core/dashboard";
import type { CoreSnapshot } from "@/core/core-components";
import { UsageEfficiency } from "./UsageEfficiency";

const chartConfig = { turns: { label: "Interações", color: "#61afef" } };
export function DashboardOverview({ data, issues, beadsError, components, onSelectSession, onOpenBoard }: {
  data: ProjectMetrics; issues: Bead[] | null; beadsError: string | null;
  components: CoreSnapshot["items"];
  onSelectSession: (id: string) => void; onOpenBoard: () => void;
}) {
  const m = data.metrics;
  const measured = m.measuredSteps + (m.efficiency?.auxiliaryRequests ?? 0) > 0;
  const models = Object.entries(m.models).sort((a,b) => b[1] - a[1]);
  const [today] = useState(() => Math.floor(Date.now() / 86_400_000));
  const days = Array.from({ length: 30 }, (_, i) => {
    const day = today - 29 + i;
    return { day: new Date(day * 86_400_000).toLocaleDateString("pt-BR", { day: "2-digit", month: "short", timeZone: "UTC" }), turns: m.days[day] ?? 0 };
  });
  const activeDays = days.filter(day => day.turns > 0).length;
  const closed = issues?.filter(issue => issue.status === "closed").length ?? 0;
  const epics = issues?.filter(issue => issue.issue_type === "epic").length ?? 0;
  const coreCalls = [
    { label: "Context-mode", icon: Layers3, color: "#56b6c2", count: Object.entries(m.tools).filter(([name]) => /^(ctx_|context_)/.test(name)).reduce((sum, [, count]) => sum + count, 0) },
    { label: "Ponytail", icon: Terminal, color: "#98c379", count: null },
    { label: "Beads", icon: Workflow, color: "#c678dd", count: Object.entries(m.tools).filter(([name]) => name.startsWith("beads_")).reduce((sum, [, count]) => sum + count, 0) },
  ];
  return <div className="mx-auto max-w-[1600px] space-y-5 p-5 @3xl:p-6">
    {data.unavailableSessions > 0 && <p role="status" className="text-xs text-onedark-yellow">Histórico indisponível em {data.unavailableSessions} sessões. Indicadores parciais.</p>}
    <div className="dashboard-metric-rail grid grid-cols-2 @3xl:grid-cols-4">
      {[{ label: "Sessões", value: number(data.sessions), sub: `${number(m.turns)} interações`, icon: MessageSquare, accent: "text-primary" },
        { label: "Tokens", value: measured ? number(m.inputTokens + m.outputTokens) : "—", sub: measured ? "Acumulados · entrada + saída" : "Sem medição", icon: Zap, accent: "text-onedark-yellow" },
        { label: "Ferramentas", value: number(m.toolCalls), sub: `${number(m.toolErrors)} com erro`, icon: Terminal, accent: "text-onedark-cyan" },
        { label: "Tarefas fechadas", value: issues ? `${closed}/${issues.length}` : "—", sub: issues ? `${epics} ${epics === 1 ? "épico" : "épicos"} no projeto` : "Beads indisponível", icon: CheckCheck, accent: "text-onedark-green" }].map(item => <div key={item.label} className="dashboard-metric min-w-0 p-4 @3xl:p-5"><div className="mb-4 flex items-center justify-between gap-2"><span className="micro-label text-muted-foreground">{item.label}</span><item.icon className={`size-4 ${item.accent}`} /></div><p className="font-mono text-[28px] leading-none tracking-tight tabular-nums">{item.value}</p><p className="mt-2 text-[11px] text-muted-foreground">{item.sub}</p></div>)}
    </div>

    <div className="grid gap-5 @3xl:grid-cols-[minmax(0,1.7fr)_minmax(240px,1fr)]">
      <Card className="dashboard-card gap-3"><CardHeader className="flex flex-row items-center justify-between"><div><CardTitle className="text-sm">Ritmo do projeto</CardTitle><p className="mt-1 text-xs text-muted-foreground">Últimos 30 dias · UTC</p></div><Badge variant="outline" className="font-mono text-[10px]">{activeDays} {activeDays === 1 ? "dia ativo" : "dias ativos"}</Badge></CardHeader><CardContent className="min-w-0">
        <ChartContainer config={chartConfig} className="h-[190px] w-full" aria-label={`${days.reduce((sum, day) => sum + day.turns, 0)} interações nos últimos 30 dias`}>
          <AreaChart data={days} accessibilityLayer margin={{ top: 10, right: 5, bottom: 0, left: -28 }}>
            <CartesianGrid vertical={false} strokeDasharray="3 5" /><XAxis dataKey="day" tickLine={false} axisLine={false} minTickGap={48} tickMargin={12} fontSize={10} /><YAxis allowDecimals={false} axisLine={false} tickLine={false} fontSize={10} />
            <ChartTooltip content={<ChartTooltipContent />} /><Area dataKey="turns" type="monotone" stroke="var(--color-turns)" fill="var(--color-turns)" fillOpacity={0.08} strokeWidth={2} isAnimationActive={false} />
          </AreaChart>
        </ChartContainer>
      </CardContent></Card>
      <Card className="dashboard-card gap-4"><CardHeader><CardTitle className="flex items-center gap-2 text-sm"><Zap className="size-4 text-onedark-yellow" />Tokens acumulados</CardTitle></CardHeader><CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-3"><div><p className="micro-label mb-2 flex items-center gap-1 text-muted-foreground"><ArrowDownLeft className="size-3" />Entrada</p><p className="font-mono text-xl">{measured ? number(m.inputTokens) : "—"}</p></div><div><p className="micro-label mb-2 flex items-center gap-1 text-muted-foreground"><ArrowUpRight className="size-3" />Saída</p><p className="font-mono text-xl">{measured ? number(m.outputTokens) : "—"}</p></div></div>
        <div className="space-y-3 border-t border-border pt-4"><MetricLine label="Compactações" value={m.compactions} /><MetricLine label="Etapas com tokens medidos" value={m.measuredSteps} /><MetricLine label="Tempo de execução" value={m.durationMs < 60_000 ? `${number(m.durationMs / 1000)}s` : `${number(m.durationMs / 60_000)} min`} /></div>
      </CardContent></Card>
    </div>

    <UsageEfficiency metrics={m} />
    <div className="grid gap-5 @3xl:grid-cols-2">
      <Card className="dashboard-card gap-3"><CardHeader className="flex flex-row items-center justify-between"><CardTitle className="flex items-center gap-2 text-sm"><Cpu className="size-4 text-primary" />Modelos utilizados</CardTitle><span className="micro-label text-muted-foreground">Interações</span></CardHeader><CardContent className="max-h-64 space-y-4 overflow-y-auto">
        {models.length ? models.map(([model, count]) => <div key={model} className="space-y-2"><div className="flex items-start gap-3"><div className="min-w-0 flex-1"><p title={model} className="truncate font-mono text-xs">{model.slice(model.indexOf("/") + 1)}</p><p className="mt-1 truncate text-[10px] text-muted-foreground">{model.slice(0, model.indexOf("/"))}</p></div><span className="font-mono text-xs text-muted-foreground">{number(count)}</span></div><Progress value={count / Math.max(1, m.turns) * 100} className="h-1" aria-label={`${model}: ${count} interações`} /></div>) : <QuietEmpty text="Nenhum modelo utilizado" />}
      </CardContent></Card>
      <Card className="dashboard-card gap-3"><CardHeader className="flex flex-row items-center justify-between"><CardTitle className="flex items-center gap-2 text-sm"><Workflow className="size-4 text-onedark-purple" />Fluxo de trabalho</CardTitle><Button variant="ghost" size="sm" className="h-6 cursor-pointer text-xs" onClick={onOpenBoard}>Ver quadro<ChevronRight className="size-3" /></Button></CardHeader><CardContent>
        {beadsError ? <p role="status" className="text-xs text-onedark-yellow">{beadsError}</p> : issues ? <><div className="mb-5 flex items-baseline gap-2"><span className="font-mono text-3xl">{issues.length ? Math.round(closed / issues.length * 100) : 0}<span className="text-lg text-muted-foreground">%</span></span><span className="text-xs text-muted-foreground">concluído</span></div><div className="mb-4 flex h-2 gap-0.5 overflow-hidden rounded-sm" aria-hidden="true">{issues.length ? statuses.map(status => { const count = issues.filter(issue => issue.status === status.id).length; return count > 0 && <div key={status.id} style={{ backgroundColor: status.color, flex: count }} />; }) : <div className="w-full bg-muted" />}</div><div className="grid grid-cols-2 gap-x-6 gap-y-3">{statuses.filter(status => ["open", "in_progress", "blocked", "closed"].includes(status.id) || issues.some(issue => issue.status === status.id)).map(status => <div key={status.id} className="flex items-center gap-2 text-xs"><span className="size-1.5 rounded-full" style={{ background: status.color }} /><span className="flex-1 text-muted-foreground">{status.label}</span><span className="font-mono">{issues.filter(issue => issue.status === status.id).length}</span></div>)}</div></> : <div role="status" aria-label="Carregando indicadores do Beads" className="space-y-4"><Skeleton className="h-9 w-24" /><Skeleton className="h-2" /><Skeleton className="h-16" /></div>}
      </CardContent></Card>
    </div>

    <Card className="dashboard-card gap-3"><CardHeader><CardTitle className="text-sm">Jarvis Core</CardTitle></CardHeader><CardContent className="grid grid-cols-3 gap-4">{coreCalls.map(item => { const component = components.find(component => component.name === item.label); return <div key={item.label} className="flex min-w-0 items-start gap-3 rounded-md border border-border bg-background/35 p-3"><item.icon className="mt-0.5 size-4 shrink-0" style={{ color: item.color }} /><div className="min-w-0 flex-1"><p className="truncate text-xs">{item.label}</p><p className="mt-2 font-mono text-lg">{item.count === null ? component?.installed ? "Full" : "—" : number(item.count)}</p><p className="mt-1 truncate text-[10px] text-muted-foreground">{item.count === null ? "Diretrizes" : "Chamadas no chat"}{component?.installedVersion ? ` · v${component.installedVersion}` : ""}</p></div></div>; })}</CardContent></Card>
    <Card className="dashboard-card gap-1"><CardHeader className="flex flex-row items-center justify-between"><CardTitle className="flex items-center gap-2 text-sm"><Activity className="size-4 text-muted-foreground" />Sessões recentes</CardTitle><span className="micro-label text-muted-foreground">Última atividade</span></CardHeader><CardContent className="space-y-1">{data.recent.length ? data.recent.map(session => <Button key={session.id} variant="ghost" className="h-auto w-full cursor-pointer justify-start gap-3 py-3 text-left" onClick={() => onSelectSession(session.id)}><MessageSquare className="size-4 text-muted-foreground" /><span className="min-w-0 flex-1 truncate text-xs">{session.title}</span><span className="shrink-0 font-mono text-[10px] font-normal text-muted-foreground">{date(session.activity, true)}</span><ChevronRight className="size-3 text-muted-foreground" /></Button>) : <QuietEmpty text="Nenhuma conversa ainda" />}</CardContent></Card>
  </div>;
}
function MetricLine({ label, value }: { label: string; value: number | string }) { return <div className="flex items-center justify-between gap-3 text-xs"><span className="text-muted-foreground">{label}</span><span className="font-mono">{typeof value === "number" ? number(value) : value}</span></div>; }
function QuietEmpty({ text }: { text: string }) { return <p className="py-6 text-center text-xs text-muted-foreground">{text}</p>; }
