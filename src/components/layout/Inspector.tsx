import { useState, type ReactNode } from "react";
import { ChevronRight, ListChecks, Files, Users, ClipboardCheck, ListTodo, Sparkles } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { LibrarySnapshot } from "@/core/library";
import type { ChatSnapshot } from "@/core/chat";
import type { ProviderAccount } from "@/core/provider-accounts";
import { conversationContext } from "@/core/inspector";
import { ChangedFiles } from "./ChangedFiles";
import { ContextUsage } from "./ContextUsage";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";
import { useSessionFiles } from "@/hooks/use-session-files";
import { Skeleton } from "@/components/ui/skeleton";
import { EpicPlans } from "./EpicPlans";
import { WorkflowAgents } from "./WorkflowAgents";
import { WorkflowValidation } from "./WorkflowValidation";
import { activeAgent, ROLE_COLORS } from "@/core/workflow";
import type { WorkflowController } from "@/hooks/use-workflow";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ProjectExplorer } from "@/components/files/ProjectExplorer";
import type { ProjectFilesController } from "@/hooks/use-project-files";
import { DirectTasks } from "./DirectTasks";

function ActivitySection({ title, icon, count, children }: { title: string; icon: ReactNode; count?: number; children: ReactNode }) {
  const { layout, updateLayout } = useDesktopLayout();
  return <Collapsible open={layout.activitySections[title] ?? true} onOpenChange={open => updateLayout(current => ({ activitySections: { ...current.activitySections, [title]: open } }))} className="inspector-section border-b border-border py-1.5">
    <CollapsibleTrigger render={<Button variant="ghost" />} className="group h-9 w-full cursor-pointer justify-start gap-2 px-2 text-xs">
      {icon}<span>{title}</span>{count !== undefined && <Badge variant="secondary" className="px-1.5 text-[10px]">{count}</Badge>}<ChevronRight aria-hidden="true" className="ml-auto size-3 text-muted-foreground group-aria-expanded:rotate-90" />
    </CollapsibleTrigger>
    <CollapsibleContent className="px-2 pb-3 pt-2">{children}</CollapsibleContent>
  </Collapsible>;
}

export function Inspector({ library, chat, workflow, accounts = [], onCompact, onOpenKanban, onPublish, canPublish, compacting = false, pending = false, files: fileWorkspace }: { library: LibrarySnapshot | null; chat?: ChatSnapshot | null; workflow?: WorkflowController; accounts?: ProviderAccount[]; onCompact?: () => Promise<boolean>; onOpenKanban?: (projectId: string) => void; onPublish?: () => Promise<boolean>; canPublish?: boolean; compacting?: boolean; pending?: boolean; files?: ProjectFilesController }) {
  const { layout, updateLayout } = useDesktopLayout();
  const selectedChat = chat?.conversationId === library?.selection.conversationId ? chat : null;
  const [explorerVisited, setExplorerVisited] = useState(layout.inspectorTab === "explorer");
  const [publishing, setPublishing] = useState(false);
  const turns = selectedChat?.turns ?? [];
  const latestTurn = turns[turns.length - 1];
  const hasPublishModel = canPublish ?? turns.some(turn => turn.options.workflow !== "publication");
  const liveFlow = selectedChat && workflow?.data?.conversationId === selectedChat.conversationId ? workflow.data.flow : undefined;
  const latestFlow = latestTurn?.options.workflow ?? (latestTurn?.options.mode === "plan" ? "planned" : "standard");
  const publishingFlow = latestFlow === "publication";
  const flow = liveFlow === "publication" && !publishingFlow ? latestFlow : liveFlow ?? latestFlow;
  const individualAgent = latestTurn?.options.workflow === "custom" && Boolean(latestTurn.options.customAgentId);
  const directFlow = !publishingFlow && (flow === "standard" || flow === "designer" || individualAgent);
  const manualValidation = Boolean(latestTurn?.options.manualValidation || workflow?.data?.validation && !workflow.data.validation.stale);
  const tools = turns.flatMap(turn => turn.steps.flatMap(step => step.tools));
  const changes = useSessionFiles(selectedChat?.conversationId ?? null);
  const files = changes.files;
  const projectId = library?.selection.projectId;
  const projectPath = library?.projects.find(project => project.id === projectId)?.path;
  const section = layout.inspectorTab === "explorer" ? "explorer" : "activities";
  const selectSection = (value: unknown) => { if (value === "activities" || value === "explorer") { if (value === "explorer") setExplorerVisited(true); updateLayout({ inspectorTab: value }); } };
  const tabClass = "h-7 flex-none cursor-pointer px-2.5 text-xs data-active:bg-primary/15 data-active:text-primary dark:data-active:border-primary/20 dark:data-active:bg-primary/15 dark:data-active:text-primary";
  return <aside aria-label="Inspector" className="flex h-full min-h-0 flex-col bg-sidebar">
    <Tabs value={section} onValueChange={selectSection} className="h-full min-h-0 gap-0">
      <div className="shrink-0 border-b border-border bg-card px-2 py-1"><TabsList aria-label="Seções da lateral direita" className="h-7 justify-start gap-1 rounded-md bg-transparent p-0"><TabsTrigger value="activities" className={tabClass}>Inspector</TabsTrigger><TabsTrigger value="explorer" className={tabClass}>Explorer</TabsTrigger></TabsList></div>
      <TabsContent value="activities" keepMounted className={`min-h-0 flex-1 flex-col ${section === "activities" ? "flex" : "hidden"}`}>
    <div className="min-h-0 flex-1">
        <ScrollArea className="h-full"><div key={selectedChat?.conversationId ?? "empty"} className="px-2">
          {!publishingFlow && (directFlow ? <ActivitySection title="Tarefas" icon={<ListTodo aria-hidden="true" style={flow === "designer" ? { color: ROLE_COLORS.designer } : undefined} className={`size-4 ${flow === "standard" ? "text-primary" : ""}`} />} count={latestTurn?.tasks.length}>
            <DirectTasks tasks={latestTurn?.tasks ?? []} active={!!latestTurn && selectedChat?.activeTurnId === latestTurn.id} flow={flow === "designer" ? "designer" : flow === "custom" ? "custom" : "standard"} />
          </ActivitySection> : <ActivitySection title="Plano" icon={<ListChecks aria-hidden="true" className="size-4 text-[#c678dd]" />}>
            {projectId && selectedChat ? <EpicPlans key={`${projectId}:${selectedChat.conversationId}`} projectId={projectId} conversationId={selectedChat.conversationId} active={Boolean(selectedChat.activeTurnId)} onOpenKanban={onOpenKanban} /> : <p className="text-xs text-muted-foreground">Nenhum plano em aberto.</p>}
          </ActivitySection>)}
          <ActivitySection title="Arquivos alterados" icon={<Files aria-hidden="true" className="size-4 text-primary" />} count={files.length}>
            {changes.loading ? <div role="status" aria-label="Conferindo alterações" className="space-y-2"><Skeleton className="h-8 w-full" /><Skeleton className="h-8 w-4/5" /></div> : changes.error ? <p role="alert" className="text-xs text-destructive">{changes.error}</p> : files.length && selectedChat ? <ChangedFiles key={selectedChat.conversationId} files={files} conversationId={selectedChat.conversationId} projectPath={projectPath} /> : <p className="text-xs text-muted-foreground">Nenhuma alteração pendente.</p>}
            {tools.some(tool => tool.name === "bash" || tool.name.startsWith("mcp_")) && <p className="mt-3 text-[11px] leading-relaxed text-muted-foreground">Alterações feitas pelo terminal ou por MCPs ainda não entram nesta lista.</p>}
          </ActivitySection>
          {selectedChat && files.length > 0 && <div className="border-b border-border px-2 py-3"><Button variant="outline" className="w-full cursor-pointer gap-2 border-primary/25 bg-primary/5 text-primary hover:bg-primary/10" disabled={!onPublish || !hasPublishModel || !!selectedChat.activeTurnId || compacting || pending || publishing} title={!hasPublishModel ? "Configure o modelo do agente GitHub em Configurações → Agentes" : selectedChat.activeTurnId ? "Aguarde a execução atual terminar" : "Preparar uma publicação com o agente GitHub"} onClick={() => { if (!onPublish || publishing) return; setPublishing(true); void onPublish().finally(() => setPublishing(false)); }}><Sparkles aria-hidden="true" className="size-4" />{publishing ? "Preparando…" : "Publicar"}</Button></div>}
          {selectedChat && workflow?.data?.conversationId === selectedChat.conversationId && (publishingFlow || !individualAgent && ["planned", "complete", "custom"].includes(workflow.data.flow)) && <ActivitySection title="Subagentes" icon={<Users aria-hidden="true" className="size-4 text-onedark-yellow" />}>
            <WorkflowAgents workflow={workflow} conversationId={selectedChat?.conversationId} />
          </ActivitySection>}
          {selectedChat && manualValidation && !directFlow && workflow?.data?.conversationId === selectedChat.conversationId && ["planned", "complete", "custom"].includes(workflow.data.flow) && <ActivitySection title="Validação" icon={<ClipboardCheck aria-hidden="true" className="size-4 text-onedark-green" />} count={workflow.data.validation?.items.length}>
            <WorkflowValidation key={`${selectedChat.conversationId}/${workflow.data.validation?.id ?? "empty"}`} conversationId={selectedChat.conversationId} batch={workflow.data.validation} busy={!!selectedChat.activeTurnId || compacting || pending || (selectedChat.queuedMessages?.length ?? 0) > 0 || workflow.data.agents.some(activeAgent)} onRefresh={workflow.retry} />
          </ActivitySection>}
        </div></ScrollArea>
    </div>
    <ContextUsage key={selectedChat?.conversationId ?? "empty"} context={conversationContext(turns, accounts)} live={selectedChat?.context} onCompact={onCompact} compacting={compacting} disabled={!selectedChat || turns.length === 0 || !!selectedChat.activeTurnId || pending} />
      </TabsContent>
      <TabsContent value="explorer" keepMounted className={`min-h-0 flex-1 ${section === "explorer" ? "block" : "hidden"}`}>
        {projectId && fileWorkspace && (explorerVisited || section === "explorer") ? <ProjectExplorer key={projectId} projectId={projectId} projectName={library?.projects.find(project => project.id === projectId)?.name ?? "Projeto"} selected={fileWorkspace.tabs.activePath} onOpen={fileWorkspace.open} /> : section === "explorer" ? <p className="p-4 text-xs text-muted-foreground">Abra uma conversa para explorar os arquivos do projeto.</p> : null}
      </TabsContent>
    </Tabs>
  </aside>;
}
