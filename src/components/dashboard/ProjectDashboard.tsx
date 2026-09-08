import { useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LayoutDashboard, Kanban, RefreshCw, FolderGit2, FolderOpen } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { libraryError, type Project } from "@/core/library";
import { boardSchema, projectMetricsSchema } from "@/core/dashboard";
import { useDashboardQuery } from "@/hooks/use-dashboard-query";
import { useCore } from "@/hooks/use-core";
import { DashboardOverview } from "./DashboardOverview";
import { BeadsBoard } from "./BeadsBoard";
import { DashboardSkeleton } from "./DashboardSkeleton";
import "./dashboard.css";

export function ProjectDashboard({ project, onSelectSession, navigation, initialTab = "general" }: { project: Project; onSelectSession: (id: string) => void; navigation?: ReactNode; initialTab?: "general" | "beads" }) {
  const [tab, setTab] = useState<string>(initialTab);
  const core = useCore();
  const metrics = useDashboardQuery("get_project_metrics", project.id, projectMetricsSchema);
  const board = useDashboardQuery("get_project_beads", project.id, boardSchema);
  const loading = metrics.loading || board.loading;
  return <main aria-label={`Dashboard de ${project.name}`} className="project-dashboard @container flex h-full min-h-0 flex-col overflow-hidden">
    <header className="flex shrink-0 items-center gap-3 border-b border-border px-6 py-5">
      {navigation}
      <div className="dashboard-project-mark"><FolderGit2 className="size-5" /></div>
      <div className="min-w-0 flex-1"><p className="micro-label mb-1 text-muted-foreground">Projeto / Dashboard</p><h1 className="truncate text-xl font-semibold tracking-tight">{project.name}</h1><Button variant="ghost" size="sm" className="mt-0.5 h-auto max-w-full cursor-pointer justify-start gap-1.5 px-0 py-0.5 font-mono text-[11px] text-muted-foreground hover:bg-transparent hover:text-primary" title="Abrir no Finder/Explorador" aria-label={`Abrir pasta do projeto ${project.name}`} onClick={() => { void invoke("open_project_directory", { projectId: project.id }).catch(error => toast.error(libraryError(error, "Não foi possível abrir a pasta do projeto."))); }}><FolderOpen aria-hidden="true" className="size-3 shrink-0" /><span className="truncate">{project.path}</span></Button></div>
      <Button variant="ghost" size="icon" className="cursor-pointer" aria-label="Atualizar Dashboard" title="Atualizar" disabled={loading} onClick={() => { void metrics.refresh(); void board.refresh(); }}><RefreshCw className="size-4" /></Button>
    </header>
    <Tabs value={tab} onValueChange={value => setTab(String(value))} className="min-h-0 flex-1 gap-0">
      <div className="flex shrink-0 items-center border-b border-border px-6 py-3"><TabsList className="h-9 gap-1 rounded-md border border-border/70 bg-sidebar p-1"><TabsTrigger value="general" className="h-7 cursor-pointer gap-2 rounded-sm px-3 text-xs data-active:bg-secondary data-active:shadow-sm"><LayoutDashboard />Geral</TabsTrigger><TabsTrigger value="beads" className="h-7 cursor-pointer gap-2 rounded-sm px-3 text-xs data-active:bg-secondary data-active:shadow-sm"><Kanban />Kanban{board.data && <Badge variant="secondary" className="font-mono text-[10px]">{board.data.length}</Badge>}</TabsTrigger></TabsList></div>
      <TabsContent value="general" className="min-h-0 overflow-y-auto">
        {metrics.error && <DashboardError message={metrics.error} retry={metrics.refresh} />}
        {!metrics.data ? !metrics.error && <DashboardSkeleton /> : <DashboardOverview data={metrics.data} issues={board.data} beadsError={board.error} components={core.snapshot?.items ?? []} onSelectSession={onSelectSession} onOpenBoard={() => setTab("beads")} />}
      </TabsContent>
      <TabsContent value="beads" className="flex min-h-0 flex-col overflow-hidden">
        {board.error && <DashboardError message={board.error} retry={board.refresh} />}
        {!board.data ? !board.error && <DashboardSkeleton board /> : <BeadsBoard projectName={project.name} projectId={project.id} issues={board.data} onChanged={board.refresh} />}
      </TabsContent>
    </Tabs>
  </main>;
}

export function DashboardError({ message, retry }: { message: string; retry: () => Promise<unknown> }) {
  return <div role="alert" className="m-4 flex items-center gap-3 rounded-md border border-destructive/25 bg-destructive/5 p-3 text-xs"><span className="flex-1 text-destructive">{message}</span><Button className="cursor-pointer" variant="outline" size="sm" onClick={() => { void retry(); }}>Tentar novamente</Button></div>;
}
