import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
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
import { useAgentActivity } from "@/hooks/use-agent-activity";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";

const SettingsDialog = lazy(() => import("@/components/settings/SettingsDialog").then(module => ({ default: module.SettingsDialog })));

export function Home() {
  const { layout, updateLayout } = useDesktopLayout();
  const library = useLibrary();
  const chat = useChat(library.snapshot?.selection.conversationId ?? null);
  const runningConversationIds = useAgentActivity();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const accountsVersion = useRef(0);
  const updateAccounts = useCallback((updated: ProviderAccount[]) => {
    accountsVersion.current += 1;
    setAccounts(updated);
  }, []);

  useEffect(() => {
    let active = true;
    const version = accountsVersion.current;
    void invoke<ProviderAccount[]>("list_provider_accounts").then(
      (result) => {
        if (active && version === accountsVersion.current) setAccounts(accountList(result));
      },
      () => {
        if (active && version === accountsVersion.current) setAccounts([]);
      },
    );
    return () => {
      active = false;
    };
  }, []);

  const modelGroups = accounts
    .filter((account) => account.enabled && account.modelsAvailable && account.models.length > 0)
    .map((account) => ({
      provider: account.alias,
      models: account.models.map((model) => ({
        value: `${account.alias}/${model.id}`,
        label: model.name,
        reasoningLevels: model.reasoningLevels,
        defaultReasoningLevel: model.defaultReasoningLevel,
      })),
    }));

  return (
    <div
      data-testid="home-shell"
      className="desktop-shell dark flex h-full min-h-0 w-full min-w-[1024px] overflow-hidden bg-background font-sans text-foreground"
    >
      <ResizablePanelGroup
        id="home-shell-panels"
        orientation="horizontal"
        defaultLayout={Object.keys(layout.panels).length ? layout.panels : undefined}
        onLayoutChanged={(panels) => {
          // Native resizing also changes the constrained proportions. Persist what is visible.
          if (Object.keys(panels).some(id => Math.abs(panels[id] - (layout.panels[id] ?? -1)) > 0.001)) updateLayout({ panels });
        }}
        className="h-full min-h-0 w-full flex-1"
      >
        <ResizablePanel
          id="home-sidebar-panel"
          defaultSize="20%"
          minSize="220px"
          maxSize="500px"
          className="h-full min-h-0 min-w-0"
        >
          <AppSidebar library={library} runningConversationIds={runningConversationIds} onOpenSettings={() => setSettingsOpen(true)} />
        </ResizablePanel>

        <ResizableHandle
          aria-label="Redimensionar barra lateral"
          className="panel-separator cursor-col-resize bg-border hover:bg-primary/50"
        />

        <ResizablePanel
          id="home-main-panel"
          defaultSize="56%"
          minSize="360px"
          className="h-full min-h-0 min-w-0"
        >
          <ChatArea modelGroups={modelGroups} library={library.snapshot} chat={chat} />
        </ResizablePanel>

        <ResizableHandle
          aria-label="Redimensionar inspector"
          className="panel-separator cursor-col-resize bg-border hover:bg-primary/50"
        />

        <ResizablePanel
          id="home-inspector-panel"
          defaultSize="24%"
          minSize="280px"
          maxSize="550px"
          className="h-full min-h-0 min-w-0"
        >
          <Inspector library={library.snapshot} chat={chat.snapshot} accounts={accounts} onCompact={chat.compact} compacting={chat.compacting} pending={chat.pending} />
        </ResizablePanel>
      </ResizablePanelGroup>

      <Suspense fallback={<SettingsSkeleton open={settingsOpen} onOpenChange={setSettingsOpen} />}><SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        onAccountsChange={updateAccounts}
      /></Suspense>
    </div>
  );
}

export default Home;
