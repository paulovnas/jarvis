import { useState, type CSSProperties } from "react";
import {
  LayoutDashboard,
  ChevronRight,
  Layers,
  FolderPlus,
  MessageSquare,
  Plus,
  SquareTerminal,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Sidebar,
  SidebarContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
} from "@/components/ui/sidebar";
import { SidebarSkeleton } from "./LoadingSkeletons";
import { Spinner } from "@/components/ui/spinner";
import { Skeleton } from "@/components/ui/skeleton";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { appearanceOfProject, type Conversation, type Project } from "@/core/library";
import type { LibraryController } from "@/hooks/use-library";
import { ItemNameDialog } from "./ItemNameDialog";
import { LibraryItemMenu } from "./LibraryItemMenu";
import { DeleteItemDialog } from "./DeleteItemDialog";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";
import { SortableItem, SortableList } from "./SortableList";
import { orderedItems } from "@/core/item-order";
import { MoveProjectDialog } from "./MoveProjectDialog";
import { MoveGlobalChatDialog } from "./MoveGlobalChatDialog";
import { Hint } from "@/components/ui/hint";
import { WorkflowIdentityIcon } from "@/components/agents/WorkflowIdentityIcon";
import { WORKFLOW_COLORS } from "@/components/agents/workflow-appearance";

type NameDialog =
  | { kind: "workspace" }
  | { kind: "conversation"; item: Conversation };

type DeletionDialog =
  | { kind: "project"; item: Project }
  | { kind: "conversation"; item: Conversation };

const sidebarStyle = {
  "--sidebar-width": "100%",
  height: "100%",
  minHeight: 0,
} as CSSProperties;

const globalChatDragType = "application/x-jarvis-global-conversation";

export function AppSidebar({
  library,
  runningConversationIds,
  unreadConversationIds,
  terminalCounts,
}: {
  library: LibraryController;
  runningConversationIds?: ReadonlySet<string>;
  unreadConversationIds?: ReadonlySet<string>;
  terminalCounts?: ReadonlyMap<string, number>;
}) {
  const [dialog, setDialog] = useState<NameDialog | null>(null);
  const [moving, setMoving] = useState<Project | null>(null);
  const [movingChat, setMovingChat] = useState<Conversation | null>(null);
  const [dropProject, setDropProject] = useState<string | null>(null);
  const [visibleCounts, setVisibleCounts] = useState<Record<string, number>>({});
  const { layout, updateLayout } = useDesktopLayout();
  const expanded = layout.expandedProjects;
  const setExpanded = (update: (values: Record<string, boolean>) => Record<string, boolean>) => updateLayout(current => ({ expandedProjects: update(current.expandedProjects) }));
  const [deletion, setDeletion] = useState<DeletionDialog | null>(null);
  const { snapshot, loading, pending, error } = library;
  const selected = snapshot?.selection;
  const workspace = snapshot?.workspaces.find(
    (item) => item.id === selected?.workspaceId,
  );
  const projectOrderKey = `projects:${workspace?.id ?? ""}`;
  const projects = orderedItems(snapshot?.projects.filter((item) => item.workspaceId === workspace?.id) ?? [], layout.itemOrder[projectOrderKey], item => item.id);
  const saveOrder = (key: string, ids: string[]) => updateLayout(current => ({ itemOrder: { ...current.itemOrder, [key]: ids } }));
  const project = projects.find((item) => item.id === selected?.projectId);
  const busy = loading || pending;
  const globalChats = (snapshot?.conversations.filter(chat => chat.projectId === null) ?? []).sort((a, b) => (b.lastActivityAt ?? b.createdAt) - (a.lastActivityAt ?? a.createdAt));
  const globalCount = visibleCounts.global ?? 3;
  const openDialog = (value: NameDialog) => {
    library.clearError();
    setDialog(value);
  };
  const conversationList = (items: Conversation[], all: Conversation[], projectId: string, accent: string) => (
    <SortableList ids={items.map(item => item.id)} onReorder={ids => saveOrder(`chats:${projectId}`, [...ids, ...all.filter(item => !ids.includes(item.id)).map(item => item.id)])}>
    <SidebarMenu aria-label="Conversas do projeto">
      {items.map((item) => (
        <SortableItem key={item.id} id={item.id} disabled={busy}>{sort => <SidebarMenuItem ref={sort.setNodeRef} style={sort.style}>
          <LibraryItemMenu
            disabled={busy}
            onEdit={() => openDialog({ kind: "conversation", item })}
            onDelete={() => { library.clearError(); setDeletion({ kind: "conversation", item }); }}
          >
            <SidebarMenuButton
              ref={sort.setActivatorNodeRef}
              {...sort.listeners}
              aria-describedby={sort.attributes["aria-describedby"]}
              className="native-navigation-row h-[34px] cursor-pointer pr-8 text-[13px] text-muted-foreground data-active:text-foreground!"
              isActive={item.id === selected?.conversationId}
              aria-current={
                item.id === selected?.conversationId ? "page" : undefined
              }
              disabled={busy}
              onClick={() => {
                void library.select({ kind: "conversation", id: item.id });
              }}
            >
              {runningConversationIds?.has(item.id) ? <Spinner aria-label="Conversa em execução" className="motion-reduce:animate-none" style={{ color: accent }} /> : <MessageSquare className="text-muted-foreground" style={item.id === selected?.conversationId ? { color: accent } : undefined} />}
              <Hint content={item.title} whenTruncated><span className="min-w-0 flex-1 truncate">{item.title}</span></Hint>
              {unreadConversationIds?.has(item.id) && <Hint content="Mensagem não lida"><Badge role="img" aria-label="Mensagem não lida" className="size-2 shrink-0 rounded-full border-0 p-0" style={{ backgroundColor: accent }} /></Hint>}
            </SidebarMenuButton>
          </LibraryItemMenu>
          <Hint content="Excluir conversa"><SidebarMenuAction showOnHover aria-label={`Excluir conversa ${item.title}`} disabled={busy} className="cursor-pointer text-muted-foreground hover:text-destructive focus-visible:opacity-100" onClick={() => { library.clearError(); setDeletion({ kind: "conversation", item }); }}><Trash2 aria-hidden="true" /></SidebarMenuAction></Hint>
        </SidebarMenuItem>}</SortableItem>
      ))}
    </SidebarMenu>
    </SortableList>
  );

  return (
    <aside
      aria-label="Espaço"
      className="native-sidebar h-full min-h-0 w-full overflow-hidden border-r border-border"
    >
      <SidebarProvider
        defaultOpen
        style={sidebarStyle}
        className="h-full min-h-0 w-full"
      >
        <Sidebar collapsible="none" className="h-full w-full bg-sidebar">
          <SidebarHeader className="gap-2 border-b border-border px-3 py-3">
            <section aria-label="Global" className="mb-1 border-b border-border pb-3">
              <div className="mb-1 flex h-7 items-center justify-between px-1"><span className="text-xs font-medium text-muted-foreground">Global</span><Hint content="Nova conversa global"><Button variant="ghost" size="icon" aria-label="Nova conversa global" disabled={busy} className="size-6 cursor-pointer text-muted-foreground" onClick={() => void library.createGlobalConversation()}><Plus aria-hidden="true" className="size-3.5" /></Button></Hint></div>
              <div className="max-h-64 overflow-y-auto"><SidebarMenu aria-label="Conversas globais">
                {loading && <SidebarMenuItem role="status" aria-label="Carregando conversas globais"><div className="space-y-1">{[0, 1, 2].map(index => <div key={index} className="flex h-[34px] items-center gap-2 px-2"><Skeleton className="size-4 shrink-0" /><Skeleton className="h-3 w-3/4" /></div>)}</div></SidebarMenuItem>}
                {globalChats.slice(0, globalCount).map(item => <SidebarMenuItem key={item.id}>
                  <LibraryItemMenu disabled={busy} onEdit={() => openDialog({ kind: "conversation", item })} onDelete={() => { library.clearError(); setDeletion({ kind: "conversation", item }); }} moveLabel="Mover para um projeto" onMove={runningConversationIds?.has(item.id) ? undefined : () => { library.clearError(); setMovingChat(item); }}>
                    <SidebarMenuButton className="native-navigation-row h-[34px] cursor-pointer pr-8 text-[13px] text-muted-foreground data-active:text-foreground!" disabled={busy} isActive={item.id === selected?.conversationId} aria-current={item.id === selected?.conversationId ? "page" : undefined} draggable={!busy && !runningConversationIds?.has(item.id)} onDragStart={event => { event.dataTransfer.setData(globalChatDragType, item.id); event.dataTransfer.effectAllowed = "move"; }} onDragEnd={() => setDropProject(null)} onClick={() => void library.select({ kind: "conversation", id: item.id })}>
                      {runningConversationIds?.has(item.id) ? <Spinner aria-label="Conversa em execução" className="text-primary motion-reduce:animate-none" /> : <MessageSquare aria-hidden="true" className="text-muted-foreground" />}
                      <Hint content={item.title} whenTruncated><span className="min-w-0 flex-1 truncate">{item.title}</span></Hint>
                      {unreadConversationIds?.has(item.id) && <Badge role="img" aria-label="Mensagem não lida" className="size-2 shrink-0 rounded-full border-0 bg-primary p-0" />}
                    </SidebarMenuButton>
                  </LibraryItemMenu>
                  <Hint content="Excluir conversa"><SidebarMenuAction showOnHover aria-label={`Excluir conversa ${item.title}`} disabled={busy} className="cursor-pointer text-muted-foreground hover:text-destructive focus-visible:opacity-100" onClick={() => { library.clearError(); setDeletion({ kind: "conversation", item }); }}><Trash2 aria-hidden="true" /></SidebarMenuAction></Hint>
                </SidebarMenuItem>)}
              </SidebarMenu></div>
              {!busy && !globalChats.length && <p className="px-1 py-2 text-xs text-muted-foreground">Conversas sem projeto ficam aqui.</p>}
              {globalChats.length > globalCount && <Button variant="ghost" size="sm" className="mt-0.5 h-[30px] w-full cursor-pointer justify-start pl-8 text-xs text-muted-foreground" onClick={() => setVisibleCounts(counts => ({ ...counts, global: globalCount + 10 }))}>Ver mais<span className="ml-auto font-mono text-[11px]">+{Math.min(10, globalChats.length - globalCount)}</span></Button>}
            </section>
            <span className="flex items-center gap-2 px-1 text-xs font-medium text-muted-foreground"><Layers aria-hidden="true" className="size-3.5" />Espaço</span>
            <div className="flex items-center gap-2">
              <Select
                items={(snapshot?.workspaces ?? []).map((item) => ({
                  value: item.id,
                  label: item.name,
                }))}
                value={workspace?.id ?? null}
                disabled={busy || !snapshot?.workspaces.length}
                onValueChange={(id) => {
                  if (id) void library.select({ kind: "workspace", id });
                }}
              >
                <SelectTrigger
                  aria-label="Selecionar espaço"
                  className="h-8 w-full min-w-0 cursor-pointer border-border bg-secondary/50 text-[13px] shadow-none hover:bg-secondary"
                >
                  <SelectValue placeholder="Selecione um espaço" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {snapshot?.workspaces.map((item) => (
                      <SelectItem
                        key={item.id}
                        value={item.id}
                        className="cursor-pointer"
                      >
                        {item.name}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
            </Select>
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="ghost" size="icon" aria-label="Adicionar projeto ou espaço" className="size-8 shrink-0 cursor-pointer" disabled={busy || !snapshot} />}>
                <Hint content="Adicionar projeto ou espaço"><span className="flex items-center"><Plus /></span></Hint>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-64">
                <DropdownMenuGroup>
                  <DropdownMenuItem className="cursor-pointer" disabled={!workspace || busy} onClick={() => { if (workspace) void library.addProject(workspace.id); }}><FolderPlus />Adicionar projeto</DropdownMenuItem>
                  <DropdownMenuItem className="cursor-pointer" disabled={busy} onClick={() => openDialog({ kind: "workspace" })}><Layers />Novo espaço</DropdownMenuItem>
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
            </div>
          </SidebarHeader>
          <SidebarContent className="gap-0">
            {loading && <SidebarSkeleton />}
            {error && !dialog && !deletion && (
              <div className="space-y-2 p-3">
                <p role="alert" className="text-sm text-destructive">
                  {error}
                </p>
                <Button
                  variant="outline"
                  className="cursor-pointer"
                  disabled={busy}
                  onClick={() => {
                    void library.refresh();
                  }}
                >
                  Tentar novamente
                </Button>
              </div>
            )}
            {!loading && snapshot && (
              <div className="min-h-0 flex-1 overflow-y-auto px-2.5 py-4">
                <div className="mb-2 flex items-center justify-between px-1.5 text-xs font-medium text-muted-foreground"><span>Projetos</span><span className="font-mono text-[11px] tabular-nums">{projects.length}</span></div>
                  {!workspace ? (
                    <Empty>
                      <EmptyHeader>
                        <EmptyTitle>Organize seus projetos</EmptyTitle>
                        <EmptyDescription>
                          Crie um espaço para começar.
                        </EmptyDescription>
                      </EmptyHeader>
                    </Empty>
                  ) : (
                    <>
                      {!projects.length && (
                        <Empty>
                          <EmptyHeader>
                            <EmptyTitle>Nenhum projeto</EmptyTitle>
                            <EmptyDescription>
                              Use Novo → Adicionar projeto para selecionar uma pasta.
                            </EmptyDescription>
                          </EmptyHeader>
                        </Empty>
                      )}
                      <SortableList ids={projects.map(item => item.id)} onReorder={ids => saveOrder(projectOrderKey, ids)}><SidebarMenu aria-label="Projetos do espaço">
                        {projects.map((item) => {
                          const conversations = orderedItems(snapshot.conversations.filter(entry => entry.projectId === item.id)
                            .sort((a, b) => (b.lastActivityAt ?? b.createdAt) - (a.lastActivityAt ?? a.createdAt) || b.createdAt - a.createdAt || a.id.localeCompare(b.id)), layout.itemOrder[`chats:${item.id}`], entry => entry.id, "first");
                          const visibleCount = visibleCounts[item.id] ?? 3;
                          const isCurrentProject = item.id === project?.id;
                          const isOpen = expanded[item.id] ?? item.id === project?.id;
                          const appearance = appearanceOfProject(item);
                          const accent = WORKFLOW_COLORS[appearance.color].value;
                          return (
                          <SortableItem key={item.id} id={item.id} disabled={busy}>{sort => <SidebarMenuItem ref={sort.setNodeRef} style={sort.style} data-active-project={isCurrentProject ? "true" : undefined} data-project-color={appearance.color} className="rounded-md py-0.5">
                            <Collapsible role="group" aria-label={`Projeto ${item.name}`} aria-current={isCurrentProject ? "true" : undefined} className={dropProject === item.id ? "rounded-md bg-accent ring-1 ring-primary/50" : undefined} onDragOver={event => { if (!busy && event.dataTransfer.types.includes(globalChatDragType)) { event.preventDefault(); event.dataTransfer.dropEffect = "move"; setDropProject(item.id); } }} onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropProject(null); }} onDrop={event => { const id = event.dataTransfer.getData(globalChatDragType); setDropProject(null); if (busy || !globalChats.some(chat => chat.id === id) || runningConversationIds?.has(id)) return; event.preventDefault(); event.stopPropagation(); void library.moveGlobalConversation(id, item.id).then(saved => { if (saved) setExpanded(values => ({ ...values, [item.id]: true })); }); }} open={isOpen} onOpenChange={(open) => setExpanded(values => ({ ...values, [item.id]: open }))}>
                            <LibraryItemMenu
                              disabled={busy}
                              onDelete={() => { library.clearError(); setDeletion({ kind: "project", item }); }}
                              onMove={() => { library.clearError(); setMoving(item); }}
                            >
                              <CollapsibleTrigger render={<SidebarMenuButton
                                ref={sort.setActivatorNodeRef}
                                {...sort.listeners}
                                aria-describedby={sort.attributes["aria-describedby"]}
                                size="lg"
                                className={`native-navigation-row h-[34px] cursor-pointer text-[13px] text-muted-foreground data-active:bg-transparent! data-active:shadow-none! ${isCurrentProject ? "font-semibold text-foreground!" : ""}`}
                                isActive={item.id === project?.id}
                                disabled={busy}
                                onClick={() => {
                                  if (item.id !== project?.id) void library.select({
                                    kind: "project",
                                    id: item.id,
                                  });
                                }}
                              />}>
                                <ChevronRight aria-hidden="true" className={`size-3 text-muted-foreground transition-transform motion-reduce:transition-none ${isOpen ? "rotate-90" : ""}`} />
                                {snapshot.conversations.some(entry => entry.projectId === item.id && runningConversationIds?.has(entry.id)) ? <Spinner aria-label="Projeto com conversa em execução" className="motion-reduce:animate-none" style={{ color: accent }} /> : <WorkflowIdentityIcon appearance={appearance} className="size-4" />}
                                <Hint content={item.path}><span className="min-w-0 flex-1">
                                  <span className="block truncate">
                                    {item.name}
                                  </span>
                                </span></Hint>
                                {(terminalCounts?.get(item.id) ?? 0) > 0 && <Hint content={`${terminalCounts?.get(item.id)} ${terminalCounts?.get(item.id) === 1 ? "terminal aberto" : "terminais abertos"}`}><span role="img" aria-label={`${terminalCounts?.get(item.id)} ${terminalCounts?.get(item.id) === 1 ? "terminal aberto" : "terminais abertos"}`} className="flex shrink-0 items-center gap-0.5 font-mono text-[9px] text-onedark-green"><SquareTerminal aria-hidden="true" className="size-3.5" />{(terminalCounts?.get(item.id) ?? 0) > 1 && terminalCounts?.get(item.id)}</span></Hint>}
                                {conversations.some(entry => unreadConversationIds?.has(entry.id)) && <Hint content="Mensagens não lidas"><Badge role="img" aria-label="Projeto com mensagens não lidas" className="mr-1 size-2 shrink-0 rounded-full border-0 p-0" style={{ backgroundColor: accent }} /></Hint>}
                              </CollapsibleTrigger>
                            </LibraryItemMenu>
                              <Hint content="Nova conversa"><SidebarMenuAction
                                type="button"
                                aria-label={`Nova conversa em ${item.name}`}
                                className="cursor-pointer text-muted-foreground peer-data-[size=lg]/menu-button:top-2 disabled:pointer-events-none disabled:opacity-50"
                                disabled={busy}
                                onClick={() => {
                                  setExpanded(values => ({ ...values, [item.id]: true }));
                                  void library.createConversation(item.id);
                                }}
                              >
                                <Plus aria-hidden="true" />
                              </SidebarMenuAction></Hint>
                              <CollapsibleContent className="my-0.5 ml-2.5 border-l border-border/60 pl-1.5">
                                <SidebarMenu className="mb-1">
                                  <SidebarMenuItem>
                                    <SidebarMenuButton className="native-navigation-row h-[34px] cursor-pointer text-[13px] text-muted-foreground data-active:text-foreground!" disabled={busy}
                                      isActive={selected?.projectId === item.id && !selected.conversationId}
                                      aria-current={selected?.projectId === item.id && !selected.conversationId ? "page" : undefined}
                                      onClick={() => { void library.select({ kind: "project", id: item.id }); }}>
                                      <LayoutDashboard className="text-muted-foreground" style={selected?.projectId === item.id && !selected.conversationId ? { color: accent } : undefined} /><span>Detalhes</span>
                                    </SidebarMenuButton>
                                  </SidebarMenuItem>
                                </SidebarMenu>
                                {conversationList(conversations.slice(0, visibleCount), conversations, item.id, accent)}
                                {conversations.length > visibleCount && <Button variant="ghost" size="sm" className="mt-0.5 h-[34px] w-full cursor-pointer justify-start pl-8 text-xs text-muted-foreground" onClick={() => setVisibleCounts(counts => ({ ...counts, [item.id]: visibleCount + 10 }))}>Ver mais<span className="ml-auto font-mono text-[11px]">+{Math.min(10, conversations.length - visibleCount)}</span></Button>}
                              </CollapsibleContent>
                            </Collapsible>
                          </SidebarMenuItem>}</SortableItem>
                        ); })}
                      </SidebarMenu></SortableList>
                    </>
                  )}
              </div>
            )}
          </SidebarContent>
        </Sidebar>
      </SidebarProvider>
      {dialog && (
        <ItemNameDialog
          kind={dialog.kind}
          initialValue={
            dialog.kind === "workspace"
              ? ""
              : dialog.item.title
          }
          pending={pending}
          error={error}
          onClose={() => {
            setDialog(null);
            library.clearError();
          }}
          onSubmit={(name) =>
            dialog.kind === "workspace"
              ? library.createWorkspace(name)
              : library.renameConversation(dialog.item.id, name)
          }
        />
      )}
      {deletion && <DeleteItemDialog
        kind={deletion.kind}
        name={deletion.kind === "project" ? deletion.item.name : deletion.item.title}
        conversationCount={snapshot?.conversations.filter(item => item.projectId === deletion.item.id).length ?? 0}
        pending={pending}
        error={error}
        onClose={() => { setDeletion(null); library.clearError(); }}
        onConfirm={() => library.deleteItem({ kind: deletion.kind, id: deletion.item.id })}
      />}
      {movingChat && <MoveGlobalChatDialog conversation={movingChat} library={library} onClose={() => setMovingChat(null)} />}
      {moving && <MoveProjectDialog project={moving} workspaces={snapshot?.workspaces ?? []} library={library} onClose={() => { setMoving(null); library.clearError(); }} />}
    </aside>
  );
}
