import { useState, type ReactNode } from "react";
import { ChevronRight, ListChecks, Files, Users, ClipboardCheck, ListTodo, PanelRight, GitBranch, FolderOpen, Palette } from "lucide-react";
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
import { Hint } from "@/components/ui/hint";
import { TitleBarSlot } from "./TitleBar";
import { ProjectExplorer } from "@/components/files/ProjectExplorer";
import type { ProjectFilesController } from "@/hooks/use-project-files";
import { DirectTasks } from "./DirectTasks";
import { GithubRepositoriesPanel } from "./GithubRepositoriesPanel";
import { executorOf } from "@/core/executors";
import { DesignerPanel, type DesignerSession } from "./DesignerPanel";

export type InspectorSection = "activities" | "github" | "explorer" | "designer";
const sections = [
  { value: "activities", label: "Inspector", icon: PanelRight },
  { value: "github", label: "GitHub", icon: GitBranch },
  { value: "explorer", label: "Explorer", icon: FolderOpen },
  { value: "designer", label: "Designer", icon: Palette },
] as const;

export function InspectorNavigation({ section, collapsed, global = false, disabled = false, onSelect }: { section: InspectorSection; collapsed: boolean; global?: boolean; disabled?: boolean; onSelect: (section: InspectorSection) => void }) {
  return <TitleBarSlot side="right"><div role="group" aria-label="Lateral direita" className="flex items-center gap-0.5 px-1">
    {sections.map(({ value, label, icon: Icon }) => {
      const active = !collapsed && section === value;
      const unavailable = disabled || global && value !== "activities";
      return <Hint key={value} content={unavailable ? "Disponível em conversas de projeto" : `${active ? "Recolher" : "Abrir"} ${label}`}><Button type="button" variant="ghost" size="icon-sm" aria-label={label} aria-pressed={active} aria-expanded={active} aria-controls="inspector-panel-content" disabled={unavailable} onClick={() => onSelect(value)} className={`size-7 cursor-pointer rounded-md ${active ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground"}`}><Icon aria-hidden="true" className="size-4" /></Button></Hint>;
    })}
  </div></TitleBarSlot>;
}

function ActivitySection({ title, icon, count, children }: { title: string; icon: ReactNode; count?: number; children: ReactNode }) {
  const { layout, updateLayout } = useDesktopLayout();
  return <Collapsible open={layout.activitySections[title] ?? true} onOpenChange={open => updateLayout(current => ({ activitySections: { ...current.activitySections, [title]: open } }))} className="inspector-section border-b border-border py-1">
    <CollapsibleTrigger render={<Button variant="ghost" />} className="group h-9 w-full cursor-pointer justify-start gap-2 px-2 text-[13px] font-medium">
      {icon}<span>{title}</span>{count !== undefined && <Badge variant="secondary" className="rounded-full px-1.5 text-[11px] font-normal">{count}</Badge>}<ChevronRight aria-hidden="true" className="ml-auto size-3 text-muted-foreground group-aria-expanded:rotate-90" />
    </CollapsibleTrigger>
    <CollapsibleContent className="px-2 pb-3 pt-2">{children}</CollapsibleContent>
  </Collapsible>;
}

export function Inspector({ library, chat, workflow, accounts = [], onCompact, onOpenKanban, compacting = false, pending = false, files: fileWorkspace, designer, onSelectSection, collapsed }: { library: LibrarySnapshot | null; chat?: ChatSnapshot | null; workflow?: WorkflowController; accounts?: ProviderAccount[]; onCompact?: () => Promise<boolean>; onOpenKanban?: (projectId: string) => void; compacting?: boolean; pending?: boolean; files?: ProjectFilesController; designer?: DesignerSession; onSelectSection?: (section: InspectorSection) => void; collapsed?: boolean }) {
  const { layout, updateLayout } = useDesktopLayout();
  const selectedChat = chat?.conversationId === library?.selection.conversationId ? chat : null;
  const [explorerVisited, setExplorerVisited] = useState(layout.inspectorTab === "explorer");
  const [githubVisited, setGithubVisited] = useState(layout.inspectorTab === "github");
  const turns = selectedChat?.turns ?? [];
  const latestTurn = turns[turns.length - 1];
  const liveFlow = selectedChat && workflow?.data?.conversationId === selectedChat.conversationId ? workflow.data.flow : undefined;
  const latestFlow = latestTurn?.options.workflow ?? (latestTurn?.options.mode === "plan" ? "planned" : "standard");
  const publishingFlow = latestFlow === "publication";
  const flow = liveFlow === "publication" && !publishingFlow ? latestFlow : liveFlow ?? latestFlow;
  const individualAgent = latestTurn?.options.workflow === "custom" && Boolean(latestTurn.options.customAgentId);
  const directFlow = !publishingFlow && (flow === "standard" || flow === "designer" || flow === "video" || flow === "image_generator" || individualAgent);
  const manualValidation = Boolean(latestTurn?.options.manualValidation || workflow?.data?.validation && !workflow.data.validation.stale);
  const tools = turns.flatMap(turn => turn.steps.flatMap(step => step.tools));
  const changes = useSessionFiles(selectedChat?.conversationId ?? null);
  const files = changes.files;
  const projectId = library?.selection.projectId;
  const projectPath = library?.projects.find(project => project.id === projectId)?.path;
  const global = library?.conversations.find(conversation => conversation.id === library.selection.conversationId)?.projectId === null;
  const section = !global && (layout.inspectorTab === "explorer" || layout.inspectorTab === "github" || layout.inspectorTab === "designer") ? layout.inspectorTab : "activities";
  const selectSection = (value: InspectorSection) => { if (value === "github") setGithubVisited(true); if (value === "explorer") setExplorerVisited(true); if (onSelectSection) onSelectSection(value); else updateLayout({ inspectorTab: value }); };
  return <aside aria-label="Inspector" className="native-inspector flex h-full min-h-0 flex-col bg-sidebar">
    <InspectorNavigation section={section} collapsed={collapsed ?? layout.inspectorCollapsed} global={global} onSelect={selectSection} />
    <div className="flex h-full min-h-0 flex-col">
      <div hidden={section !== "activities"} className={`min-h-0 flex-1 flex-col ${section === "activities" ? "flex" : "hidden"}`}>
    <div className="min-h-0 flex-1">
        <ScrollArea className="h-full"><div key={selectedChat?.conversationId ?? "empty"} className="px-2">
          {!publishingFlow && (directFlow ? <ActivitySection title="Tarefas" icon={<ListTodo aria-hidden="true" style={flow === "designer" || flow === "video" || flow === "image_generator" ? { color: ROLE_COLORS[flow] } : undefined} className="size-4 text-muted-foreground" />} count={latestTurn?.tasks?.length}>
            <DirectTasks tasks={latestTurn?.tasks ?? []} active={!!latestTurn && selectedChat?.activeTurnId === latestTurn.id} flow={flow === "designer" || flow === "video" || flow === "image_generator" || flow === "custom" ? flow : "standard"} />
          </ActivitySection> : <ActivitySection title="Plano" icon={<ListChecks aria-hidden="true" className="size-4 text-muted-foreground" />}>
            {projectId && selectedChat ? <EpicPlans key={`${projectId}:${selectedChat.conversationId}`} projectId={projectId} conversationId={selectedChat.conversationId} active={Boolean(selectedChat.activeTurnId)} onOpenKanban={onOpenKanban} /> : <p className="text-xs text-muted-foreground">Nenhum plano em aberto.</p>}
          </ActivitySection>)}
          <ActivitySection title="Arquivos alterados" icon={<Files aria-hidden="true" className="size-4 text-muted-foreground" />} count={files.length}>
            {changes.loading ? <div role="status" aria-label="Conferindo alterações" className="space-y-2"><Skeleton className="h-8 w-full" /><Skeleton className="h-8 w-4/5" /></div> : changes.error ? <p role="alert" className="text-xs text-destructive">{changes.error}</p> : files.length && selectedChat ? <ChangedFiles key={selectedChat.conversationId} files={files} conversationId={selectedChat.conversationId} projectPath={projectPath ?? library?.globalDirectory} /> : <p className="text-xs text-muted-foreground">Nenhuma alteração pendente.</p>}
            {tools.some(tool => tool.name === "bash" || tool.name.startsWith("mcp_")) && <p className="mt-3 text-[11px] leading-relaxed text-muted-foreground">Alterações feitas pelo terminal ou por MCPs ainda não entram nesta lista.</p>}
          </ActivitySection>
          {selectedChat && workflow?.data?.conversationId === selectedChat.conversationId && (publishingFlow || !individualAgent && ["planned", "complete", "custom"].includes(workflow.data.flow) || workflow.data.flow === latestFlow && workflow.data.agents.some(agent => agent.parentId !== null)) && <ActivitySection title="Subagentes" icon={<Users aria-hidden="true" className="size-4 text-muted-foreground" />}>
            <WorkflowAgents workflow={workflow} conversationId={selectedChat?.conversationId} accounts={accounts} />
          </ActivitySection>}
          {selectedChat && manualValidation && !directFlow && workflow?.data?.conversationId === selectedChat.conversationId && ["planned", "complete", "custom"].includes(workflow.data.flow) && <ActivitySection title="Validação" icon={<ClipboardCheck aria-hidden="true" className="size-4 text-muted-foreground" />} count={workflow.data.validation?.items.length}>
            <WorkflowValidation key={`${selectedChat.conversationId}/${workflow.data.validation?.id ?? "empty"}`} conversationId={selectedChat.conversationId} batch={workflow.data.validation} busy={!!selectedChat.activeTurnId || compacting || pending || (selectedChat.queuedMessages?.length ?? 0) > 0 || workflow.data.agents.some(activeAgent)} onRefresh={workflow.retry} />
          </ActivitySection>}
        </div></ScrollArea>
    </div>
    <ContextUsage key={selectedChat?.conversationId ?? "empty"} context={conversationContext(turns, accounts)} live={selectedChat?.context} onCompact={onCompact} compacting={compacting} external={executorOf(selectedChat?.latestOptions ?? latestTurn?.options) !== "jarvis"} externalProvider={executorOf(selectedChat?.latestOptions ?? latestTurn?.options) === "unavailable" ? "Executor removido" : executorOf(selectedChat?.latestOptions ?? latestTurn?.options) === "muse" ? "Muse Code" : "Claude Code"} disabled={!selectedChat || turns.length === 0 || !!selectedChat.activeTurnId || pending} />
      </div>
      <div hidden={section !== "github"} className={`min-h-0 flex-1 ${section === "github" ? "block" : "hidden"}`}>
        {projectId && (githubVisited || section === "github") ? <GithubRepositoriesPanel key={projectId} projectId={projectId} active={section === "github"} /> : section === "github" ? <p className="p-4 text-xs text-muted-foreground">Abra uma conversa para consultar os repositórios do projeto.</p> : null}
      </div>
      <div hidden={section !== "explorer"} className={`min-h-0 flex-1 ${section === "explorer" ? "block" : "hidden"}`}>
        {projectId && fileWorkspace && (explorerVisited || section === "explorer") ? <ProjectExplorer key={projectId} projectId={projectId} projectName={library?.projects.find(project => project.id === projectId)?.name ?? "Projeto"} selected={fileWorkspace.tabs.activePath} onOpen={fileWorkspace.open} /> : section === "explorer" ? <p className="p-4 text-xs text-muted-foreground">Abra uma conversa para explorar os arquivos do projeto.</p> : null}
      </div>
      <div hidden={section !== "designer"} className={`min-h-0 flex-1 ${section === "designer" ? "block" : "hidden"}`}>
        {!global && <DesignerPanel key={library?.selection.conversationId ?? "empty"} session={designer?.conversationId === library?.selection.conversationId ? designer : undefined} />}
      </div>
    </div>
  </aside>;
}
