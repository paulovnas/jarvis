import { useState } from "react";
import { Bug, Check, Circle, Filter, Layers, MessageSquare, Search, SlidersHorizontal, UserRound } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Empty, EmptyHeader, EmptyTitle, EmptyDescription } from "@/components/ui/empty";
import { type Bead, actorName, date, shortId, statuses, statusFor, typeName } from "@/core/dashboard";
import { BeadDrawer } from "./BeadDrawer";

export function BeadsBoard({ projectId, projectName, issues, onChanged }: { projectId: string; projectName?: string; issues: Bead[]; onChanged: () => Promise<unknown> }) {
  const [query, setQuery] = useState("");
  const [type, setType] = useState("all");
  const [showEmpty, setShowEmpty] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [limits, setLimits] = useState<Record<string, number>>({});
  const normalized = query.trim().toLocaleLowerCase("pt-BR");
  const filtered = issues.filter(issue => (type === "all" || issue.issue_type === type) && (!normalized || [issue.id, shortId(issue.id, projectName), issue.title, issue.assignee, ...issue.labels].some(text => text.toLocaleLowerCase("pt-BR").includes(normalized))));
  const types = [...new Set(issues.map(issue => issue.issue_type))].sort();
  const allStatuses = [...statuses, ...[...new Set(issues.map(issue => issue.status))].filter(id => !statuses.some(status => status.id === id)).map(statusFor)];
  const lanes = allStatuses.filter(status => showEmpty || ["open", "in_progress", "blocked", "closed"].includes(status.id) || issues.some(issue => issue.status === status.id));
  return <>
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-5 py-3">
      <div className="relative min-w-44 max-w-80 flex-1"><Search className="pointer-events-none absolute top-2.5 left-3 size-3.5 text-muted-foreground" /><Input aria-label="Buscar tarefas" placeholder="Buscar tarefas…" value={query} onChange={event => setQuery(event.target.value)} className="h-9 pl-9 text-xs" /></div>
      <Select value={type} onValueChange={value => { if (value) setType(value); }} items={[{ value: "all", label: "Todos os tipos" }, ...types.map(value => ({ value, label: typeName(value) }))]}><SelectTrigger className="h-9 w-40 cursor-pointer text-xs" aria-label="Filtrar tipo de tarefa"><Filter className="size-3" /><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem className="cursor-pointer" value="all">Todos os tipos</SelectItem>{types.map(value => <SelectItem className="cursor-pointer" key={value} value={value}>{typeName(value)}</SelectItem>)}</SelectGroup></SelectContent></Select>
      <Button variant={showEmpty ? "secondary" : "ghost"} size="sm" className="h-9 cursor-pointer text-xs" aria-pressed={showEmpty} onClick={() => setShowEmpty(value => !value)}><SlidersHorizontal className="size-3.5" />Estados vazios</Button>
      <span className="ml-auto font-mono text-[10px] text-muted-foreground">{filtered.length === issues.length ? `${issues.length} itens` : `${filtered.length} de ${issues.length}`}</span>
    </div>
    {issues.length > 0 && !filtered.length ? <Empty className="m-6"><EmptyHeader><EmptyTitle>Nenhuma tarefa encontrada</EmptyTitle><EmptyDescription>Tente outro termo ou tipo.</EmptyDescription></EmptyHeader><Button variant="outline" className="cursor-pointer" onClick={() => { setQuery(""); setType("all"); }}>Limpar filtros</Button></Empty> : <div aria-label="Quadro Kanban" className="flex min-h-0 flex-1 gap-3 overflow-x-auto p-5">
      {lanes.map(status => {
        const cards = filtered.filter(issue => issue.status === status.id);
        const limit = limits[status.id] ?? 30;
        return <section key={status.id} aria-label={`${status.label}: ${cards.length}`} className="flex h-full min-h-60 w-[260px] min-w-[220px] flex-1 flex-col rounded-lg border border-border/70 bg-sidebar/45">
          <header className="flex h-11 shrink-0 items-center gap-2.5 border-b border-border/70 px-3"><span className="size-2 rounded-full" style={{ backgroundColor: status.color, boxShadow: `0 0 0 3px ${status.color}10` }} /><h2 className="text-xs font-medium">{status.label}</h2><span className="ml-auto font-mono text-[11px] text-muted-foreground">{cards.length}</span></header>
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

function BeadCard({ issue, projectName, onClick }: { issue: Bead; projectName?: string; onClick: () => void }) {
  const Icon = issue.issue_type === "epic" ? Layers : issue.issue_type === "bug" ? Bug : issue.status === "closed" ? Check : Circle;
  return <Button variant="ghost" className="bead-card h-auto w-full cursor-pointer flex-col items-stretch gap-3 rounded-md border border-border bg-card p-3 text-left font-normal whitespace-normal" aria-label={`${typeName(issue.issue_type)}: ${issue.title}`} onClick={onClick}>
    <div className="flex items-center gap-2"><Icon className={`size-3.5 ${issue.issue_type === "epic" ? "text-onedark-purple" : "text-muted-foreground"}`} /><span className="min-w-0 flex-1 truncate font-mono text-[10px] text-muted-foreground" title={issue.id}>{shortId(issue.id, projectName)}</span><span className={`font-mono text-[10px] ${issue.priority <= 1 ? "text-onedark-red" : "text-muted-foreground"}`}>P{issue.priority}</span></div>
    <p className="line-clamp-3 text-[13px] leading-5 font-medium">{issue.title}</p>
    {(issue.issue_type === "epic" || issue.labels.length > 0) && <div className="flex flex-wrap gap-1">{issue.issue_type === "epic" && <Badge variant="outline" className="border-onedark-purple/25 bg-onedark-purple/5 text-[9px] text-onedark-purple">Épico</Badge>}{issue.labels.slice(0, 2).map(label => <Badge key={label} variant="secondary" className="max-w-28 truncate text-[9px]">{label}</Badge>)}{issue.labels.length > 2 && <span className="text-[10px] text-muted-foreground">+{issue.labels.length - 2}</span>}</div>}
    <div className="flex items-center gap-2 border-t border-border/60 pt-2 text-[10px] text-muted-foreground"><span className="min-w-0 flex-1 truncate">{issue.assignee ? <span className="flex items-center gap-1"><UserRound className="size-3" /><span className="truncate">{actorName(issue.assignee)}</span></span> : typeName(issue.issue_type)}</span>{issue.comment_count > 0 && <span className="flex items-center gap-1"><MessageSquare className="size-3" />{issue.comment_count}</span>}<span className="shrink-0 font-mono">{date(issue.updated_at)}</span></div>
  </Button>;
}
