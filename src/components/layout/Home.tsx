import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useGroupRef } from "react-resizable-panels";
import { visiblePanels, rememberPanelResize } from "@/core/desktop-layout";
import { PanelToggle } from "./PanelToggle";
import { StatusBar } from "./StatusBar";
import { invoke } from "@tauri-apps/api/core";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { ChatArea } from "@/components/chat/ChatArea";
import { SettingsSkeleton } from "./LoadingSkeletons";
import { accountList, type ProviderAccount } from "@/core/provider-accounts";
import { Inspector } from "./Inspector";
import { AppSidebar } from "./Sidebar";
import { useLibrary } from "@/hooks/use-library";
import { useChat } from "@/hooks/use-chat";
import { useWorkflow } from "@/hooks/use-workflow";
import { useAgentModels } from "@/hooks/use-agent-models";
import { useProviderReferences } from "@/hooks/use-provider-references";
import { useAgentActivity } from "@/hooks/use-agent-activity";
import { useTerminalActivity } from "@/hooks/use-terminal-activity";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";
import { DashboardSkeleton } from "@/components/dashboard/DashboardSkeleton";
import { EmptyWorkspace } from "./EmptyWorkspace";
import { useSettingsMenu } from "@/hooks/use-settings-menu";
import { useUnreadConversations } from "@/hooks/use-unread-conversations";
import { useProjectFiles } from "@/hooks/use-project-files";
import type { ChatDraft } from "@/core/chat";
import type { QuestionDraft } from "@/core/questions";
import { useBootstrapResources } from "@/hooks/use-bootstrap-resources";

const SettingsDialog = lazy(() => import("@/components/settings/SettingsDialog").then(module => ({ default: module.SettingsDialog })));
const ProjectDashboard = lazy(() => import("@/components/dashboard/ProjectDashboard").then(module => ({ default: module.ProjectDashboard })));

export function Home() {
  const bootstrapResources = useBootstrapResources();
  const updateBootstrapAccounts = bootstrapResources?.updateAccounts;
  // Outlive ChatArea remounts when navigating through dashboards/empty workspaces.
  const [drafts] = useState(() => new Map<string, ChatDraft>());
  const [questionDrafts] = useState(() => new Map<string, QuestionDraft>());
  const { layout, updateLayout } = useDesktopLayout();
  const library = useLibrary();
  const files = useProjectFiles(library.snapshot?.selection.projectId ?? null);
  const [kanbanProjectId, setKanbanProjectId] = useState<string | null>(null);
  const sidebarLibrary = { ...library, select: (target: Parameters<typeof library.select>[0]) => {
    setKanbanProjectId(null);
    if (target.kind === "conversation") {
      const projectId = library.snapshot?.conversations.find(item => item.id === target.id)?.projectId;
      if (projectId) updateLayout(current => {
        const tabs = current.fileTabs[projectId];
        return tabs ? { fileTabs: { ...current.fileTabs, [projectId]: { ...tabs, activePath: null } } } : {};
      });
    }
    return library.select(target);
  } };
  const openKanban = (projectId: string) => {
    if (library.snapshot?.selection.projectId !== projectId) return;
    setKanbanProjectId(projectId);
    void library.select({ kind: "project", id: projectId }).then(selected => { if (!selected) setKanbanProjectId(null); });
  };
  const chat = useChat(library.snapshot?.selection.conversationId ?? null);
  const workflow = useWorkflow(library.snapshot?.selection.conversationId ?? null);
  const agentModels = useAgentModels();
  const runningConversationIds = useAgentActivity();
  const terminalCounts = useTerminalActivity();
  const publishChanges = useCallback(async () => {
    const turns = chat.snapshot?.turns ?? [];
    const options = chat.snapshot?.latestOptions ?? turns[turns.length - 1]?.options;
    if (!options) return false;
    return chat.send(
      "Prepare a publicação das alterações atuais deste projeto. Siga as opções configuradas em Detalhes → Opções, revise o diff e as validações, e apresente a proposta completa no painel para minha aprovação. Não execute commit, push, pull request ou merge por outro caminho.",
      { account: options.account, model: options.model, reasoning: options.reasoning, mode: "build", workflow: "standard", approvalMode: "yolo" },
    );
  }, [chat]);
  const dashboardProject = !library.snapshot?.selection.conversationId
    ? library.snapshot?.projects.find(project => project.id === library.snapshot?.selection.projectId)
    : undefined;
  const panelLayout = visiblePanels(layout, Boolean(dashboardProject));
  const groupRef = useGroupRef();
  const [animating, setAnimating] = useState(false);
  const layoutKey = JSON.stringify(panelLayout);
  useLayoutEffect(() => { groupRef.current?.setLayout(JSON.parse(layoutKey) as Record<string, number>); }, [groupRef, layoutKey]);
  useEffect(() => {
    if (!animating) return;
    const timer = setTimeout(() => setAnimating(false), 240);
    return () => clearTimeout(timer);
  }, [animating, layout.sidebarCollapsed, layout.inspectorCollapsed]);
  const toggle = (side: "left" | "right") => {
    setAnimating(true);
    updateLayout(side === "left" ? { sidebarCollapsed: !layout.sidebarCollapsed } : { inspectorCollapsed: !layout.inspectorCollapsed });
  };
  const leftToggle = <PanelToggle side="left" collapsed={layout.sidebarCollapsed} onToggle={() => toggle("left")} />;
  const rightToggle = <PanelToggle side="right" collapsed={layout.inspectorCollapsed} onToggle={() => toggle("right")} />;
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [latestVisibleId, setLatestVisibleId] = useState<string | null>(null);
  const onLatestVisibility = useCallback((id: string, visible: boolean) => {
    setLatestVisibleId(current => visible ? id : current === id ? null : current);
  }, []);
  const selectedId = library.snapshot?.selection.conversationId;
  const unreadConversationIds = useUnreadConversations(!settingsOpen && !files.tabs.activePath && !chat.error && chat.snapshot && latestVisibleId === selectedId ? latestVisibleId : null);
  useSettingsMenu(setSettingsOpen);
  const [localAccounts, setLocalAccounts] = useState<ProviderAccount[]>(() => bootstrapResources?.resources.accounts ?? []);
  const [localAccountsReady, setLocalAccountsReady] = useState(() => bootstrapResources?.resources.loaded.accounts ?? false);
  const accounts = bootstrapResources ? bootstrapResources.resources.accounts : localAccounts;
  const accountsReady = bootstrapResources ? bootstrapResources.resources.loaded.accounts : localAccountsReady;
  const references = useProviderReferences(accounts, accountsReady);
  const accountsVersion = useRef(0);
  const loadAccountsInitially = useRef(!(bootstrapResources?.resources.loaded.accounts ?? false));
  const updateAccounts = useCallback((updated: ProviderAccount[]) => {
    accountsVersion.current += 1;
    if (updateBootstrapAccounts) updateBootstrapAccounts(updated);
    else { setLocalAccounts(updated); setLocalAccountsReady(true); }
  }, [updateBootstrapAccounts]);

  useEffect(() => {
    if (!loadAccountsInitially.current) return;
    loadAccountsInitially.current = false;
    let active = true;
    const version = accountsVersion.current;
    void invoke<ProviderAccount[]>("list_provider_accounts").then(
      (result) => {
        if (active && version === accountsVersion.current) updateAccounts(accountList(result));
      },
      () => {
        if (active && version === accountsVersion.current) setLocalAccounts([]);
      },
    );
    return () => {
      active = false;
    };
  }, [updateAccounts]);

  const modelGroups = accounts
    .filter((account) => account.enabled && account.modelsAvailable && account.models.length > 0)
    .map((account) => ({
      provider: account.alias,
      providerKind: account.providerKind,
      models: account.models.map((model) => ({
        value: `${account.alias}/${model.id}`,
        label: model.name,
        reasoningLevels: model.reasoningLevels,
        defaultReasoningLevel: model.defaultReasoningLevel,
      })),
    }));

  const workspace = library.snapshot?.workspaces.find(item => item.id === library.snapshot?.selection.workspaceId);
  if (workspace && !library.snapshot?.projects.some(project => project.workspaceId === workspace.id)) {
    return <div data-testid="home-shell" className="desktop-shell dark flex h-full min-h-0 w-full flex-col bg-background text-foreground"><div className="flex min-h-0 flex-1"><div className="w-64 shrink-0"><AppSidebar library={sidebarLibrary} runningConversationIds={runningConversationIds} unreadConversationIds={unreadConversationIds} terminalCounts={terminalCounts} /></div><EmptyWorkspace workspace={workspace} library={library} /></div><StatusBar accounts={accounts} onOpenSettings={() => setSettingsOpen(true)} /><Suspense fallback={<SettingsSkeleton open={settingsOpen} onOpenChange={setSettingsOpen} />}><SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} onAccountsChange={updateAccounts} /></Suspense></div>;
  }

  return (
    <div
      data-testid="home-shell"
      className="desktop-shell dark flex h-full min-h-0 w-full min-w-[1024px] flex-col overflow-hidden bg-background font-sans text-foreground"
    >
      <ResizablePanelGroup
        key={dashboardProject ? "dashboard" : "chat"}
        id="home-shell-panels"
        orientation="horizontal"
        defaultLayout={panelLayout}
        groupRef={groupRef}
        onLayoutChanged={(panels, meta) => { if (meta.isUserInteraction) updateLayout(rememberPanelResize(layout, panels, Boolean(dashboardProject))); }}
        className={`h-full min-h-0 w-full flex-1 ${animating ? "panels-animating" : ""}`}
      >
        <ResizablePanel
          id="home-sidebar-panel"
          collapsible
          defaultSize="20%"
          minSize="220px"
          maxSize="500px"
          className="h-full min-h-0 min-w-0"
        >
          <div id="workspace-panel-content" inert={layout.sidebarCollapsed} aria-hidden={layout.sidebarCollapsed} className={`h-full min-w-[220px] transition-transform duration-200 motion-reduce:transition-none ${layout.sidebarCollapsed ? "-translate-x-full" : ""}`}><AppSidebar library={sidebarLibrary} runningConversationIds={runningConversationIds} unreadConversationIds={unreadConversationIds} terminalCounts={terminalCounts} /></div>
        </ResizablePanel>

        {/* Keep each separator in geometric order: display:none breaks the panel
            library's adjacency map when a collapsed layout is restored. */}
        <ResizableHandle
          aria-label="Redimensionar barra lateral"
          aria-hidden={layout.sidebarCollapsed}
          inert={layout.sidebarCollapsed}
          className={`panel-separator cursor-col-resize bg-border hover:bg-primary/50 ${layout.sidebarCollapsed ? "invisible w-0 pointer-events-none" : ""}`}
        />

        <ResizablePanel
          id="home-main-panel"
          defaultSize={dashboardProject ? "80%" : "56%"}
          minSize="360px"
          className="h-full min-h-0 min-w-0"
        >
          {dashboardProject ? <Suspense fallback={<DashboardSkeleton />}><ProjectDashboard key={dashboardProject.id} project={dashboardProject} initialTab={kanbanProjectId === dashboardProject.id ? "beads" : "general"} navigation={leftToggle} onSelectSession={id => { setKanbanProjectId(null); files.select(null); void library.select({ kind: "conversation", id }); }} /></Suspense> : <ChatArea drafts={drafts} questionDrafts={questionDrafts} onLatestVisibility={onLatestVisibility} leftToggle={leftToggle} rightToggle={rightToggle} modelGroups={modelGroups} modelBindings={references.bindings} modelsReady={accountsReady && !references.loading} library={library.snapshot} chat={chat} workflow={workflow} agentModels={agentModels} files={files} />}
        </ResizablePanel>

        {!dashboardProject && <><ResizableHandle
          aria-label="Redimensionar inspector"
          aria-hidden={layout.inspectorCollapsed}
          inert={layout.inspectorCollapsed}
          className={`panel-separator cursor-col-resize bg-border hover:bg-primary/50 ${layout.inspectorCollapsed ? "invisible w-0 pointer-events-none" : ""}`}
        />

        <ResizablePanel
          id="home-inspector-panel"
          collapsible
          defaultSize="24%"
          minSize="280px"
          maxSize="550px"
          className="h-full min-h-0 min-w-0"
        >
          <div id="inspector-panel-content" inert={layout.inspectorCollapsed} aria-hidden={layout.inspectorCollapsed} className={`h-full min-w-[280px] transition-transform duration-200 motion-reduce:transition-none ${layout.inspectorCollapsed ? "translate-x-full" : ""}`}><Inspector library={library.snapshot} chat={chat.snapshot} workflow={workflow} accounts={accounts} onOpenKanban={openKanban} onPublish={publishChanges} onCompact={chat.compact} compacting={chat.compacting} pending={chat.pending} files={files} /></div>
        </ResizablePanel></>}
      </ResizablePanelGroup>
      <StatusBar accounts={accounts} onOpenSettings={() => setSettingsOpen(true)} />

      <Suspense fallback={<SettingsSkeleton open={settingsOpen} onOpenChange={setSettingsOpen} />}><SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        onAccountsChange={updateAccounts}
      /></Suspense>
    </div>
  );
}

export default Home;
