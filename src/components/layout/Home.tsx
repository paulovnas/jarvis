import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { ChatArea } from "@/components/chat/ChatArea";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { accountList, type ProviderAccount } from "@/core/provider-accounts";
import { Inspector } from "./Inspector";
import { AppSidebar } from "./Sidebar";
import { useLibrary } from "@/hooks/use-library";
import { useChat } from "@/hooks/use-chat";
import { useAgentActivity } from "@/hooks/use-agent-activity";

export function Home() {
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
      provider: `OpenAI Codex · ${account.alias}`,
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
      className="dark flex h-full min-h-0 w-full min-w-[1024px] overflow-hidden bg-[#282c34] font-sans text-[#abb2bf]"
    >
      <ResizablePanelGroup
        id="home-shell-panels"
        orientation="horizontal"
        className="h-full min-h-0 w-full flex-1"
      >
        <ResizablePanel
          id="home-sidebar-panel"
          defaultSize="22%"
          minSize="240px"
          maxSize="500px"
          className="h-full min-h-0 min-w-0"
        >
          <AppSidebar library={library} runningConversationIds={runningConversationIds} onOpenSettings={() => setSettingsOpen(true)} />
        </ResizablePanel>

        <ResizableHandle
          withHandle
          aria-label="Redimensionar barra lateral"
          className="cursor-pointer cursor-col-resize bg-[#3e4451] hover:bg-[#61afef]/50"
        />

        <ResizablePanel
          id="home-main-panel"
          defaultSize="50%"
          minSize="360px"
          className="h-full min-h-0 min-w-0"
        >
          <ChatArea modelGroups={modelGroups} library={library.snapshot} chat={chat} />
        </ResizablePanel>

        <ResizableHandle
          withHandle
          aria-label="Redimensionar inspector"
          className="cursor-pointer cursor-col-resize bg-[#3e4451] hover:bg-[#61afef]/50"
        />

        <ResizablePanel
          id="home-inspector-panel"
          defaultSize="28%"
          minSize="360px"
          maxSize="550px"
          className="h-full min-h-0 min-w-0"
        >
          <Inspector library={library.snapshot} chat={chat.snapshot} />
        </ResizablePanel>
      </ResizablePanelGroup>

      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        onAccountsChange={updateAccounts}
      />
    </div>
  );
}

export default Home;
