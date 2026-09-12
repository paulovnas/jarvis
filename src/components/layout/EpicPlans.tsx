import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowUpRight, ChevronRight, Layers3, Trophy } from "lucide-react";
import { toast } from "sonner";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { LazyChatMarkdown as ChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import { boardSchema, dashboardError, statusFor, type Bead } from "@/core/dashboard";
import { useDashboardQuery } from "@/hooks/use-dashboard-query";
import { Hint } from "@/components/ui/hint";

function childrenOf(epic: Bead, issues: Bead[]) {
  return issues.filter(issue => issue.parent === epic.id
    || issue.dependencies.some(link => link.id === epic.id && link.dependency_type === "parent-child")
    || epic.dependents.some(link => link.id === issue.id && link.dependency_type === "parent-child"));
}

function Status({ issue }: { issue: Bead }) {
  const status = statusFor(issue.status);
  return <Badge variant="outline" className="shrink-0 text-[10px]" style={{ color: status.color, borderColor: `${status.color}40`, background: `${status.color}10` }}>{status.label}</Badge>;
}

export function EpicPlans({ projectId, conversationId, onOpenKanban, active = false }: { projectId: string; conversationId: string; onOpenKanban?: (projectId: string) => void; active?: boolean }) {
  return <ProjectPlans key={`${projectId}:${conversationId}`} projectId={projectId} conversationId={conversationId} onOpenKanban={onOpenKanban} active={active} />;
}

function CompletedPlan({ issue, onFinish }: { issue: Bead; onFinish: (id: string) => void }) {
  useEffect(() => {
    const timer = window.setTimeout(() => onFinish(issue.id), 2200);
    return () => window.clearTimeout(timer);
  }, [issue.id, onFinish]);
  return <div role="status" aria-label={`Plano finalizado: ${issue.title}`} className="epic-completed relative overflow-hidden rounded-md border border-onedark-green/40 bg-onedark-green/10 p-3 shadow-[inset_0_1px_0_#ffffff0a]">
    <div className="flex items-center gap-2 text-onedark-green"><Trophy aria-hidden="true" className="size-4" /><span className="font-mono text-[11px] font-semibold tracking-[0.18em]">FINALIZADO</span></div>
    <p className="mt-2 line-clamp-2 text-xs leading-5 text-foreground">{issue.title}</p>
  </div>;
}

function ProjectPlans({ projectId, conversationId, onOpenKanban, active }: { projectId: string; conversationId: string; onOpenKanban?: (projectId: string) => void; active: boolean }) {
  const board = useDashboardQuery("get_project_beads", projectId, boardSchema);
  const [selected, setSelected] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [closing, setClosing] = useState(false);
  const [closeError, setCloseError] = useState<string | null>(null);
  const [display, setDisplay] = useState<{ source: Bead[] | null; items: Bead[] }>({ source: null, items: [] });
  if (board.data && !board.error && display.source !== board.data) {
    // Only an observed open -> closed transition earns a completion animation.
    setDisplay({ source: board.data, items: board.data.filter(issue => issue.issue_type === "epic" && issue.metadata.jarvis_conversation === conversationId && (issue.status !== "closed" || display.items.some(item => item.id === issue.id))) });
  }
  const finishCompletion = useCallback((id: string) => {
    setDisplay(current => ({ ...current, items: current.items.filter(item => item.id !== id || item.status !== "closed") }));
  }, []);
  const epics = (board.data ?? []).filter(issue => issue.issue_type === "epic" && issue.status !== "closed" && issue.metadata.jarvis_conversation === conversationId);
  const epic = epics.find(issue => issue.id === selected);
  if (selected && ((board.data && !epic) || board.error)) setSelected(null);
  const tasks = epic ? childrenOf(epic, board.data ?? []) : [];
  const description = epic?.description.trim().split(/\n\s*\n/)[0] ?? "";
  const closePlan = async () => {
    if (!epic || closing) return;
    setClosing(true); setCloseError(null);
    try {
      boardSchema.parse(await invoke("close_conversation_plan", { projectId, conversationId, issueId: epic.id }));
      setConfirming(false); setSelected(null);
      toast.success("Plano encerrado.");
      await board.refresh();
    } catch (cause) { setCloseError(dashboardError(cause)); }
    finally { setClosing(false); }
  };
  return <>
    {board.error ? <div role="alert" className="space-y-2 text-xs"><p className="text-destructive">{board.error}</p><Button variant="ghost" size="sm" className="cursor-pointer" onClick={() => void board.refresh()}>Tentar novamente</Button></div>
      : !board.data ? <div role="status" aria-label="Carregando planos" className="space-y-2"><Skeleton className="h-20 w-full rounded-md" /><Skeleton className="h-20 w-full rounded-md" /></div>
      : !display.items.length ? <p className="text-xs text-muted-foreground">Nenhum plano em aberto.</p>
      : <div className="space-y-2">{display.items.map(item => {
        if (item.status === "closed") return <CompletedPlan key={item.id} issue={item} onFinish={finishCompletion} />;
        const children = childrenOf(item, board.data ?? []);
        const done = children.filter(child => child.status === "closed").length;
        return <Button key={item.id} variant="ghost" aria-label={`Plano: ${item.title}`} onClick={() => setSelected(item.id)} className="group h-auto w-full cursor-pointer flex-col items-stretch gap-2.5 rounded-md border border-border bg-card/60 p-3 text-left whitespace-normal shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] hover:border-[#c678dd]/40 hover:bg-[#c678dd]/5">
          <span className="flex items-center gap-2"><Layers3 aria-hidden="true" className="size-3.5 shrink-0 text-[#c678dd]" /><span className="line-clamp-2 flex-1 text-xs font-medium leading-5">{item.title}</span><ChevronRight aria-hidden="true" className="size-3 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5 motion-reduce:transition-none" /></span>
          <span className="flex items-center justify-between gap-2"><Status issue={item} /><span className="font-mono text-[10px] text-muted-foreground">{done}/{children.length} tarefas</span></span>
          {!!children.length && <Progress aria-label={`Progresso de ${item.title}`} value={done / children.length * 100} className="[&_[data-slot=progress-indicator]]:bg-[#c678dd]" />}
        </Button>;
      })}</div>}
    <Dialog open={!!epic && !board.error} onOpenChange={open => { if (!open) setSelected(null); }}>
      {epic && <DialogContent className="dark flex max-h-[min(680px,85dvh)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl">
        <DialogHeader className="shrink-0 border-b border-border bg-sidebar p-5 pr-12"><DialogDescription className="micro-label text-[#c678dd]">Plano / Épico</DialogDescription><DialogTitle className="text-base leading-6">{epic.title}</DialogTitle><div className="mt-2 flex items-center gap-2"><Status issue={epic} /><span className="font-mono text-[11px] text-muted-foreground">P{epic.priority}</span></div></DialogHeader>
        <div className="min-h-0 flex-1 space-y-5 overflow-y-auto p-5">
          {description && <div className="text-xs [&_p]:leading-6"><ChatMarkdown content={description.length > 600 ? `${description.slice(0, 600).trimEnd()}…` : description} /></div>}
          <section aria-label="Tarefas do plano"><h3 className="micro-label mb-3 flex justify-between text-muted-foreground"><span>Tarefas</span><span className="font-mono">{tasks.filter(item => item.status === "closed").length}/{tasks.length}</span></h3>
            {tasks.length ? <ul className="divide-y divide-border rounded-md border border-border bg-card/50">{tasks.map(task => <li key={task.id} className="flex items-start gap-3 px-3 py-3"><span className="min-w-0 flex-1 text-xs leading-5">{task.title}</span><Status issue={task} /></li>)}</ul> : <p className="text-xs text-muted-foreground">Nenhuma tarefa vinculada.</p>}
          </section>
        </div>
        <DialogFooter className="mx-0 mb-0 shrink-0 border-t border-border bg-sidebar px-5 pt-5 pb-6 sm:justify-between">
          <Hint content={active ? "Interrompa a execução atual antes de encerrar o plano." : undefined}><Button variant="destructive" size="sm" className="cursor-pointer text-xs" disabled={active || closing} onClick={() => { setCloseError(null); setConfirming(true); }}>Encerrar plano</Button></Hint>
          {onOpenKanban && <Button variant="outline" size="sm" className="cursor-pointer gap-2 text-xs" onClick={() => { setSelected(null); onOpenKanban(projectId); }}>Ver mais detalhes<ArrowUpRight className="size-3.5" /></Button>}
        </DialogFooter>
      </DialogContent>}
    </Dialog>
    <AlertDialog open={confirming} onOpenChange={open => { if (!closing) { setConfirming(open); if (!open) setCloseError(null); } }}>
      <AlertDialogContent className="dark">
        <AlertDialogHeader>
          <AlertDialogTitle>Encerrar este plano?</AlertDialogTitle>
          <AlertDialogDescription>O épico e suas {tasks.filter(task => task.status !== "closed").length} tarefas ainda abertas serão encerrados. Arquivos já alterados não serão desfeitos.</AlertDialogDescription>
        </AlertDialogHeader>
        {closeError && <p role="alert" className="text-xs text-destructive">{closeError}</p>}
        <AlertDialogFooter>
          <AlertDialogCancel className="cursor-pointer" disabled={closing}>Manter plano</AlertDialogCancel>
          <AlertDialogAction data-confirm-action variant="destructive" className="cursor-pointer" disabled={closing} onClick={event => { event.preventDefault(); void closePlan(); }}>{closing ? "Encerrando…" : "Encerrar plano"}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </>;
}
