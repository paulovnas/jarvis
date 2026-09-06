import type { ReactNode } from "react";
import { ChevronRight, ListChecks, Files, Users, ClipboardCheck } from "lucide-react";
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
import { activeAgent } from "@/core/workflow";
import type { WorkflowController } from "@/hooks/use-workflow";

function ActivitySection({ title, icon, count, children }: { title: string; icon: ReactNode; count?: number; children: ReactNode }) {
  const { layout, updateLayout } = useDesktopLayout();
  return <Collapsible open={layout.activitySections[title] ?? true} onOpenChange={open => updateLayout(current => ({ activitySections: { ...current.activitySections, [title]: open } }))} className="inspector-section border-b border-border py-1.5">
    <CollapsibleTrigger render={<Button variant="ghost" />} className="group h-9 w-full cursor-pointer justify-start gap-2 px-2 text-xs">
      {icon}<span>{title}</span>{count !== undefined && <Badge variant="secondary" className="px-1.5 text-[10px]">{count}</Badge>}<ChevronRight aria-hidden="true" className="ml-auto size-3 text-muted-foreground group-aria-expanded:rotate-90" />
    </CollapsibleTrigger>
    <CollapsibleContent className="px-2 pb-3 pt-2">{children}</CollapsibleContent>
  </Collapsible>;
}

export function Inspector({ library, chat, workflow, accounts = [], onCompact, onOpenKanban, compacting = false, pending = false }: { library: LibrarySnapshot | null; chat?: ChatSnapshot | null; workflow?: WorkflowController; accounts?: ProviderAccount[]; onCompact?: () => Promise<boolean>; onOpenKanban?: (projectId: string) => void; compacting?: boolean; pending?: boolean }) {
  const selectedChat = chat?.conversationId === library?.selection.conversationId ? chat : null;
  const turns = selectedChat?.turns ?? [];
  const tools = turns.flatMap(turn => turn.steps.flatMap(step => step.tools));
  const changes = useSessionFiles(selectedChat?.conversationId ?? null);
  const files = changes.files;
  const projectId = library?.selection.projectId;
  return <aside aria-label="Inspector" className="flex h-full min-h-0 flex-col bg-sidebar">
    <div className="min-h-0 flex-1">
        <ScrollArea className="h-full"><div key={selectedChat?.conversationId ?? "empty"} className="px-2">
          <ActivitySection title="Plano" icon={<ListChecks aria-hidden="true" className="size-4 text-[#c678dd]" />}>
            {projectId ? <EpicPlans key={projectId} projectId={projectId} onOpenKanban={onOpenKanban} /> : <p className="text-xs text-muted-foreground">Nenhum plano em aberto.</p>}
          </ActivitySection>
          <ActivitySection title="Arquivos alterados" icon={<Files aria-hidden="true" className="size-4 text-primary" />} count={files.length}>
            {changes.loading ? <div role="status" aria-label="Conferindo alterações" className="space-y-2"><Skeleton className="h-8 w-full" /><Skeleton className="h-8 w-4/5" /></div> : changes.error ? <p role="alert" className="text-xs text-destructive">{changes.error}</p> : files.length && selectedChat ? <ChangedFiles key={selectedChat.conversationId} files={files} conversationId={selectedChat.conversationId} /> : <p className="text-xs text-muted-foreground">Nenhuma alteração pendente.</p>}
            {tools.some(tool => tool.name === "bash" || tool.name.startsWith("mcp_")) && <p className="mt-3 text-[11px] leading-relaxed text-muted-foreground">Alterações feitas pelo terminal ou por MCPs ainda não entram nesta lista.</p>}
          </ActivitySection>
          {selectedChat && workflow?.data?.conversationId === selectedChat.conversationId && ["planned", "complete"].includes(workflow.data.flow) && <ActivitySection title="Subagentes" icon={<Users aria-hidden="true" className="size-4 text-[#e5c07b]" />}>
            <WorkflowAgents workflow={workflow} conversationId={selectedChat?.conversationId} />
          </ActivitySection>}
          {selectedChat && workflow?.data?.conversationId === selectedChat.conversationId && ["planned", "complete"].includes(workflow.data.flow) && <ActivitySection title="Validação" icon={<ClipboardCheck aria-hidden="true" className="size-4 text-onedark-green" />} count={workflow.data.validation?.items.length}>
            <WorkflowValidation key={`${selectedChat.conversationId}/${workflow.data.validation?.id ?? "empty"}`} conversationId={selectedChat.conversationId} batch={workflow.data.validation} busy={!!selectedChat.activeTurnId || compacting || pending || (selectedChat.queuedMessages?.length ?? 0) > 0 || workflow.data.agents.some(activeAgent)} onRefresh={workflow.retry} />
          </ActivitySection>}
        </div></ScrollArea>
    </div>
    <ContextUsage key={selectedChat?.conversationId ?? "empty"} context={conversationContext(turns, accounts)} live={selectedChat?.context} onCompact={onCompact} compacting={compacting} disabled={!selectedChat || turns.length === 0 || !!selectedChat.activeTurnId || pending} />
  </aside>;
}
