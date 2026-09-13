import { useState } from "react";
import type { DateRange } from "react-day-picker";
import { ptBR } from "date-fns/locale";
import { Bug, CalendarDays, Check, ChevronDown, Circle, Filter, Layers, MessageSquare, Search, SlidersHorizontal, UserRound } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Input } from "@/components/TextInput";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Empty, EmptyHeader, EmptyTitle, EmptyDescription } from "@/components/ui/empty";
import { type Bead, actorName, date, shortId, statuses, statusFor, typeName } from "@/core/dashboard";
import { BeadDrawer } from "./BeadDrawer";
import { Hint } from "@/components/ui/hint";

type ClosedDateFilter = "today" | "3days" | "7days" | "custom";

const closedFilterLabels: Record<ClosedDateFilter, string> = {
  today: "Hoje",
  "3days": "3 Dias",
  "7days": "7 Dias",
  custom: "Personalizado",
};

function dayBoundary(date: Date, end = false) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + (end ? 1 : 0)).getTime() - (end ? 1 : 0);
}

function closedInRange(issue: Bead, filter: ClosedDateFilter, customRange?: DateRange) {
  if (!issue.closed_at) return false;
  const closedAt = new Date(issue.closed_at).getTime();
  if (!Number.isFinite(closedAt)) return false;
  const today = new Date();
  const start = filter === "custom"
    ? customRange?.from
    : new Date(today.getFullYear(), today.getMonth(), today.getDate() - (filter === "7days" ? 6 : filter === "3days" ? 2 : 0));
  const finish = filter === "custom" ? (customRange?.to ?? customRange?.from) : today;
  return Boolean(start && finish && closedAt >= dayBoundary(start) && closedAt <= dayBoundary(finish, true));
}

export function BeadsBoard({ projectId, projectName, issues, onChanged }: { projectId: string; projectName?: string; issues: Bead[]; onChanged: () => Promise<unknown> }) {
  const [query, setQuery] = useState("");
  const [type, setType] = useState("all");
  const [showEmpty, setShowEmpty] = useState(false);
  const [closedFilter, setClosedFilter] = useState<ClosedDateFilter>("today");
  const [closedRange, setClosedRange] = useState<DateRange>();
  const [selected, setSelected] = useState<string | null>(null);
  const [limits, setLimits] = useState<Record<string, number>>({});
  const normalized = query.trim().toLocaleLowerCase("pt-BR");
  const filtered = issues.filter(issue => (type === "all" || issue.issue_type === type) && (!normalized || [issue.id, shortId(issue.id, projectName), issue.title, issue.assignee, ...issue.labels].some(text => text.toLocaleLowerCase("pt-BR").includes(normalized))));
  const types = [...new Set(issues.map(issue => issue.issue_type))].sort();
  const allStatuses = [...statuses, ...[...new Set(issues.map(issue => issue.status))].filter(id => !statuses.some(status => status.id === id)).map(statusFor)];
  const lanes = allStatuses.filter(status => showEmpty || ["open", "in_progress", "blocked", "closed"].includes(status.id) || issues.some(issue => issue.status === status.id));
  const visibleCount = filtered.filter(issue => issue.status !== "closed" || closedInRange(issue, closedFilter, closedRange)).length;
  return <>
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-5 py-3">
      <div className="relative min-w-44 max-w-80 flex-1"><Search className="pointer-events-none absolute top-2.5 left-3 size-3.5 text-muted-foreground" /><Input aria-label="Buscar tarefas" placeholder="Buscar tarefas…" value={query} onChange={event => setQuery(event.target.value)} className="h-9 pl-9 text-xs" /></div>
      <Select value={type} onValueChange={value => { if (value) setType(value); }} items={[{ value: "all", label: "Todos os tipos" }, ...types.map(value => ({ value, label: typeName(value) }))]}><SelectTrigger className="h-9 w-40 cursor-pointer text-xs" aria-label="Filtrar tipo de tarefa"><Filter className="size-3" /><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem className="cursor-pointer" value="all">Todos os tipos</SelectItem>{types.map(value => <SelectItem className="cursor-pointer" key={value} value={value}>{typeName(value)}</SelectItem>)}</SelectGroup></SelectContent></Select>
      <Button variant={showEmpty ? "secondary" : "ghost"} size="sm" className="h-9 cursor-pointer text-xs" aria-pressed={showEmpty} onClick={() => setShowEmpty(value => !value)}><SlidersHorizontal className="size-3.5" />Estados vazios</Button>
      <span className="ml-auto font-mono text-[10px] text-muted-foreground">{visibleCount === issues.length ? `${issues.length} itens` : `${visibleCount} de ${issues.length}`}</span>
    </div>
    {issues.length > 0 && !filtered.length ? <Empty className="m-6"><EmptyHeader><EmptyTitle>Nenhuma tarefa encontrada</EmptyTitle><EmptyDescription>Tente outro termo ou tipo.</EmptyDescription></EmptyHeader><Button variant="outline" className="cursor-pointer" onClick={() => { setQuery(""); setType("all"); }}>Limpar filtros</Button></Empty> : <div aria-label="Quadro Kanban" className="flex min-h-0 flex-1 gap-3 overflow-x-auto p-5">
      {lanes.map(status => {
        const cards = filtered.filter(issue => issue.status === status.id && (status.id !== "closed" || closedInRange(issue, closedFilter, closedRange)));
        const limit = limits[status.id] ?? 30;
        return <section key={status.id} aria-label={`${status.label}: ${cards.length}`} className="flex h-full min-h-60 w-[260px] min-w-[220px] flex-1 flex-col rounded-lg border border-border/70 bg-sidebar/45">
          <header className="flex h-11 shrink-0 items-center gap-2.5 border-b border-border/70 px-3"><span className="size-2 rounded-full" style={{ backgroundColor: status.color, boxShadow: `0 0 0 3px ${status.color}10` }} /><h2 className="text-xs font-medium">{status.label}</h2>{status.id === "closed" && <ClosedFilter value={closedFilter} range={closedRange} onChange={(value, range) => { setClosedFilter(value); setClosedRange(range); setLimits(current => ({ ...current, closed: 30 })); }} />}<span className="ml-auto font-mono text-[11px] text-muted-foreground">{cards.length}</span></header>
          <div className="min-h-0 flex-1 space-y-2 overflow-y-auto p-2">
            {cards.slice(0, limit).map(issue => <BeadCard key={issue.id} issue={issue} projectName={projectName} onClick={() => setSelected(issue.id)} />)}
            {!cards.length && <p className="py-10 text-center text-[11px] text-muted-foreground/65">Sem tarefas</p>}
            {cards.length > limit && <Button variant="ghost" className="w-full cursor-pointer text-xs" onClick={() => setLimits(current => ({ ...current, [status.id]: limit + 30 }))}>Ver mais ({cards.length - limit})</Button>}
          </div>
        </section>;
      })}
    </div>}
    {selected && <BeadDrawer key={`${projectId}/${selected}`} projectId={projectId} projectName={projectName} issueId={selected} onClose={() => setSelected(null)} onSelect={setSelected} onChanged={onChanged} />}
  </>;
}

function ClosedFilter({ value, range, onChange }: { value: ClosedDateFilter; range?: DateRange; onChange: (value: ClosedDateFilter, range?: DateRange) => void }) {
  const [open, setOpen] = useState(false);
  const [draftRange, setDraftRange] = useState<DateRange | undefined>(range);
  const format = (date: Date) => date.toLocaleDateString("pt-BR", { day: "2-digit", month: "short", year: "numeric" });
  return <Popover open={open} onOpenChange={next => { setOpen(next); if (next) setDraftRange(range); }}>
    <PopoverTrigger render={<Button type="button" variant="outline" size="xs" />} aria-label={`Filtrar tarefas fechadas: ${closedFilterLabels[value]}`} className="h-5 cursor-pointer gap-1 rounded-full border-onedark-green/25 bg-onedark-green/5 px-2 font-mono text-[9px] font-normal text-onedark-green hover:bg-onedark-green/10 hover:text-onedark-green">
      <CalendarDays className="size-3" />{closedFilterLabels[value]}<ChevronDown className="size-2.5 opacity-70" />
    </PopoverTrigger>
    <PopoverContent align="end" className="dark w-auto gap-3 border-border bg-card p-3">
      <div className="grid grid-cols-3 gap-1" aria-label="Período das tarefas fechadas">
        {(["today", "3days", "7days"] as const).map(option => <Button key={option} type="button" size="sm" variant={value === option ? "secondary" : "ghost"} className="cursor-pointer text-xs" onClick={() => { onChange(option); setOpen(false); }}>{closedFilterLabels[option]}</Button>)}
      </div>
      <Button type="button" size="sm" variant={value === "custom" ? "secondary" : "ghost"} className="w-full cursor-pointer justify-start text-xs" onClick={() => setDraftRange(range ?? { from: new Date(), to: new Date() })}><CalendarDays className="size-3.5" />Personalizado</Button>
      {draftRange && <div className="space-y-3 border-t border-border pt-3">
        <Calendar mode="range" locale={ptBR} selected={draftRange} onSelect={setDraftRange} defaultMonth={draftRange.from} disabled={{ after: new Date() }} numberOfMonths={2} className="rounded-md border border-border bg-sidebar" />
        <div className="flex items-center justify-between gap-3">
          <span className="text-[10px] text-muted-foreground">{draftRange.from ? format(draftRange.from) : "Data inicial"}{draftRange.to && ` – ${format(draftRange.to)}`}</span>
          <Button type="button" size="sm" disabled={!draftRange.from} className="cursor-pointer" onClick={() => { if (!draftRange.from) return; onChange("custom", { from: draftRange.from, to: draftRange.to ?? draftRange.from }); setOpen(false); }}>Aplicar</Button>
        </div>
      </div>}
    </PopoverContent>
  </Popover>;
}

function BeadCard({ issue, projectName, onClick }: { issue: Bead; projectName?: string; onClick: () => void }) {
  const Icon = issue.issue_type === "epic" ? Layers : issue.issue_type === "bug" ? Bug : issue.status === "closed" ? Check : Circle;
  return <Button variant="ghost" className="bead-card h-auto w-full cursor-pointer flex-col items-stretch gap-3 rounded-md border border-border bg-card p-3 text-left font-normal whitespace-normal" aria-label={`${typeName(issue.issue_type)}: ${issue.title}`} onClick={onClick}>
    <div className="flex items-center gap-2"><Icon className={`size-3.5 ${issue.issue_type === "epic" ? "text-onedark-purple" : "text-muted-foreground"}`} /><Hint content={issue.id}><span className="min-w-0 flex-1 truncate font-mono text-[10px] text-muted-foreground">{shortId(issue.id, projectName)}</span></Hint><span className={`font-mono text-[10px] ${issue.priority <= 1 ? "text-onedark-red" : "text-muted-foreground"}`}>P{issue.priority}</span></div>
    <p className="line-clamp-3 text-[13px] leading-5 font-medium">{issue.title}</p>
    {(issue.issue_type === "epic" || issue.labels.length > 0) && <div className="flex flex-wrap gap-1">{issue.issue_type === "epic" && <Badge variant="outline" className="border-onedark-purple/25 bg-onedark-purple/5 text-[9px] text-onedark-purple">Épico</Badge>}{issue.labels.slice(0, 2).map(label => <Badge key={label} variant="secondary" className="max-w-28 truncate text-[9px]">{label}</Badge>)}{issue.labels.length > 2 && <span className="text-[10px] text-muted-foreground">+{issue.labels.length - 2}</span>}</div>}
    <div className="flex items-center gap-2 border-t border-border/60 pt-2 text-[10px] text-muted-foreground"><span className="min-w-0 flex-1 truncate">{issue.assignee ? <span className="flex items-center gap-1"><UserRound className="size-3" /><span className="truncate">{actorName(issue.assignee)}</span></span> : typeName(issue.issue_type)}</span>{issue.comment_count > 0 && <span className="flex items-center gap-1"><MessageSquare className="size-3" />{issue.comment_count}</span>}<span className="shrink-0 font-mono">{date(issue.updated_at)}</span></div>
  </Button>;
}
