import type { ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { ScrollArea } from "@/components/ui/scroll-area";
import { DirectTasks } from "@/components/layout/DirectTasks";
import type { DirectTask } from "@/core/chat";
import { cn } from "@/lib/utils";

const dots = {
  pending: "border border-muted-foreground/60",
  in_progress: "bg-primary motion-safe:animate-pulse",
  completed: "bg-onedark-green",
  blocked: "bg-destructive",
};

export function CompanionTaskProgress({ tasks, active, open, onOpenChange, children }: {
  tasks: DirectTask[]; active: boolean; open: boolean; onOpenChange: (open: boolean) => void; children: ReactNode;
}) {
  if (!tasks.length) return <div className="mt-auto flex items-center gap-2">{children}</div>;
  const completed = tasks.filter(task => task.status === "completed").length;
  const pending = tasks.filter(task => task.status === "pending").length;
  const blocked = tasks.filter(task => task.status === "blocked").length;
  const summary = `${completed} de ${tasks.length} tarefas concluídas, ${tasks.filter(task => task.status === "in_progress").length} em andamento, ${pending} pendente${pending === 1 ? "" : "s"}, ${blocked} bloqueada${blocked === 1 ? "" : "s"}`;
  return <Collapsible open={open} onOpenChange={onOpenChange} className={cn("mt-auto flex min-h-0 flex-col gap-2", open && "flex-1")}>
    <div className="flex min-w-0 items-center justify-between gap-2">
      {children}
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} aria-label={`Tarefas: ${summary}`} className="h-5 shrink-0 cursor-pointer gap-1.5 px-1 text-[10px] text-muted-foreground">
        <span aria-hidden="true" className="flex items-center gap-0.5">{tasks.slice(0, 6).map(task => <span key={task.id} className={cn("size-1.5 rounded-full", dots[task.status])} />)}{tasks.length > 6 && <span className="font-mono text-[9px]">+{tasks.length - 6}</span>}</span>
        <span aria-hidden="true" className="font-mono tabular-nums">{completed}/{tasks.length}</span>
        <ChevronDown aria-hidden="true" className={cn("size-3 transition-transform motion-reduce:transition-none", open && "rotate-180")} />
      </CollapsibleTrigger>
    </div>
    <CollapsibleContent className="min-h-0 flex-1 overflow-hidden">
      <ScrollArea className="h-full pr-2"><DirectTasks tasks={tasks} active={active} flow="standard" /></ScrollArea>
    </CollapsibleContent>
  </Collapsible>;
}
