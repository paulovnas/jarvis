import type { ReactNode } from "react";
import { Activity, ChevronRight, Folder, ListChecks, Files, Info, Users } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { LibrarySnapshot } from "@/core/library";
import type { ChatSnapshot } from "@/core/chat";
import type { ProviderAccount } from "@/core/provider-accounts";
import { conversationContext } from "@/core/inspector";
import { ChangedFiles } from "./ChangedFiles";
import { ContextUsage } from "./ContextUsage";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";

function ActivitySection({ title, icon, count, children }: { title: string; icon: ReactNode; count?: number; children: ReactNode }) {
  const { layout, updateLayout } = useDesktopLayout();
  return <Collapsible open={layout.activitySections[title] ?? true} onOpenChange={open => updateLayout(current => ({ activitySections: { ...current.activitySections, [title]: open } }))} className="inspector-section border-b border-border py-1.5">
    <CollapsibleTrigger render={<Button variant="ghost" />} className="group h-9 w-full cursor-pointer justify-start gap-2 px-2 text-xs">
      {icon}<span>{title}</span>{count !== undefined && <Badge variant="secondary" className="px-1.5 text-[10px]">{count}</Badge>}<ChevronRight aria-hidden="true" className="ml-auto size-3 text-muted-foreground group-aria-expanded:rotate-90" />
    </CollapsibleTrigger>
    <CollapsibleContent className="px-2 pb-3 pt-2">{children}</CollapsibleContent>
  </Collapsible>;
}

export function Inspector({ library, chat, accounts = [], onCompact, compacting = false, pending = false }: { library: LibrarySnapshot | null; chat?: ChatSnapshot | null; accounts?: ProviderAccount[]; onCompact?: () => Promise<boolean>; compacting?: boolean; pending?: boolean }) {
  const { layout, updateLayout } = useDesktopLayout();
  const selectedChat = chat?.conversationId === library?.selection.conversationId ? chat : null;
  const turns = selectedChat?.turns ?? [];
  const last = turns[turns.length - 1];
  const tools = turns.flatMap(turn => turn.steps.flatMap(step => step.tools));
  const workspace = library?.workspaces.find(item => item.id === library.selection.workspaceId);
  const project = library?.projects.find(item => item.id === library.selection.projectId);
  const conversation = library?.conversations.find(item => item.id === library.selection.conversationId);
  const files = selectedChat?.fileChanges ?? [];
  const status = selectedChat?.pendingQuestion ? "Aguardando resposta" : selectedChat?.pendingApproval ? "Aguardando autorização" : last ? ({ running: "Em andamento", completed: "Concluída", cancelled: "Interrompida", interrupted: "Interrompida ao encerrar", error: "Falhou" })[last.status] : null;
  return <aside aria-label="Inspector" className="flex h-full min-h-0 flex-col bg-sidebar">
    <Tabs value={layout.inspectorTab} onValueChange={value => { if (value === "details" || value === "activities") updateLayout({ inspectorTab: value }); }} className="min-h-0 flex-1 gap-0">
      <TabsList variant="line" className="h-12 w-full shrink-0 justify-start gap-4 rounded-none border-b border-border px-4">
        <TabsTrigger value="details" className="cursor-pointer gap-2 text-xs"><Info aria-hidden="true" className="size-3.5" />Detalhes</TabsTrigger>
        <TabsTrigger value="activities" className="cursor-pointer gap-2 text-xs"><Activity aria-hidden="true" className="size-3.5" />Atividades</TabsTrigger>
      </TabsList>
      <TabsContent value="details" className="m-0 min-h-0 flex-1">
        <ScrollArea className="h-full"><div className="space-y-5 p-4">
          <section aria-label="Seleção atual">
            <h3 className="micro-label mb-4 flex items-center gap-2 text-muted-foreground"><Folder aria-hidden="true" className="size-3.5 text-primary" />Projeto selecionado</h3>
            {workspace ? <dl className="space-y-4 text-xs">
              <div><dt className="text-muted-foreground">Workspace</dt><dd className="mt-1 break-words">{workspace.name}</dd></div>
              {project && <><div><dt className="text-muted-foreground">Projeto</dt><dd className="mt-1 break-words">{project.name}</dd></div><div><dt className="text-muted-foreground">Pasta</dt><dd className="mt-1 break-all font-mono text-[11px]">{project.path}</dd></div></>}
              {conversation && <div><dt className="text-muted-foreground">Conversa</dt><dd className="mt-1 break-words">{conversation.title}</dd></div>}
            </dl> : <p className="text-xs text-muted-foreground">Nenhum workspace selecionado.</p>}
          </section>
          {last && <section aria-label="Execução atual" className="space-y-3 border-t border-border pt-4 text-xs">
            <h3 className="micro-label text-muted-foreground">Última execução</h3>
            <Badge variant="outline" className={last.status === "completed" ? "border-[#98c379]/30 bg-[#98c379]/10 text-[#98c379]" : last.status === "error" ? "border-destructive/30 text-destructive" : "border-[#e5c07b]/30 text-[#e5c07b]"}>{status}</Badge>
            <p className="break-all font-mono text-[10px] text-muted-foreground">{last.options.account} / {last.options.model}</p>
            <p>{last.options.mode === "plan" ? "Plan · Somente leitura" : `Build · ${last.options.approvalMode === "manual" ? "Manual" : "YOLO"}`}</p>
            {last.options.reasoning && <p>Raciocínio: {last.options.reasoning}</p>}
            {last.status !== "running" && <p className="font-mono text-[11px] tabular-nums">Duração: {(last.durationMs / 1000).toFixed(1)}s</p>}
          </section>}
        </div></ScrollArea>
      </TabsContent>
      <TabsContent value="activities" className="m-0 min-h-0 flex-1">
        <ScrollArea className="h-full"><div key={selectedChat?.conversationId ?? "empty"} className="px-2">
          <ActivitySection title="Plano" icon={<ListChecks aria-hidden="true" className="size-4 text-[#c678dd]" />}>
            <p className="text-xs text-muted-foreground">Nenhum plano.</p>
          </ActivitySection>
          <ActivitySection title="Arquivos alterados" icon={<Files aria-hidden="true" className="size-4 text-primary" />} count={files.length}>
            {files.length && selectedChat ? <ChangedFiles key={selectedChat.conversationId} files={files} conversationId={selectedChat.conversationId} /> : <p className="text-xs text-muted-foreground">Nenhuma alteração registrada.</p>}
            {tools.some(tool => tool.name === "bash" || tool.name.startsWith("mcp_")) && <p className="mt-3 text-[11px] leading-relaxed text-muted-foreground">Alterações feitas pelo terminal ou por MCPs ainda não entram nesta lista.</p>}
          </ActivitySection>
          <ActivitySection title="Subagentes" icon={<Users aria-hidden="true" className="size-4 text-[#e5c07b]" />}>
            <p className="text-xs text-muted-foreground">Nenhum subagente.</p>
          </ActivitySection>
        </div></ScrollArea>
      </TabsContent>
    </Tabs>
    <ContextUsage key={selectedChat?.conversationId ?? "empty"} context={conversationContext(turns, accounts)} live={selectedChat?.context} onCompact={onCompact} compacting={compacting} disabled={!selectedChat || turns.length === 0 || !!selectedChat.activeTurnId || pending} />
  </aside>;
}
