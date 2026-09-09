import { useState, type CSSProperties } from "react";
import {
  Folder,
  LayoutDashboard,
  ChevronRight,
  Layers,
  FolderPlus,
  MessageSquare,
  Plus,
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
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import type { Conversation, Project } from "@/core/library";
import type { LibraryController } from "@/hooks/use-library";
import { ItemNameDialog } from "./ItemNameDialog";
import { LibraryItemMenu } from "./LibraryItemMenu";
import { DeleteItemDialog } from "./DeleteItemDialog";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";
import { SortableItem, SortableList } from "./SortableList";
import { orderedItems } from "@/core/item-order";
import { MoveProjectDialog } from "./MoveProjectDialog";

type NameDialog =
  | { kind: "workspace" }
  | { kind: "project"; item: Project }
  | { kind: "conversation"; item: Conversation };

const sidebarStyle = {
  "--sidebar-width": "100%",
  height: "100%",
  minHeight: 0,
} as CSSProperties;

export function AppSidebar({
  library,
  runningConversationIds,
  unreadConversationIds,
}: {
  library: LibraryController;
  runningConversationIds?: ReadonlySet<string>;
  unreadConversationIds?: ReadonlySet<string>;
}) {
  const [dialog, setDialog] = useState<NameDialog | null>(null);
  const [moving, setMoving] = useState<Project | null>(null);
  const [visibleCounts, setVisibleCounts] = useState<Record<string, number>>({});
  const { layout, updateLayout } = useDesktopLayout();
  const expanded = layout.expandedProjects;
  const setExpanded = (update: (values: Record<string, boolean>) => Record<string, boolean>) => updateLayout(current => ({ expandedProjects: update(current.expandedProjects) }));
  const [deletion, setDeletion] = useState<Exclude<NameDialog, { kind: "workspace" }> | null>(null);
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
  const openDialog = (value: NameDialog) => {
    library.clearError();
    setDialog(value);
  };
  const conversationList = (items: Conversation[], all: Conversation[], projectId: string) => (
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
              className="h-7 cursor-pointer pr-8 text-muted-foreground/80 data-active:text-foreground!"
              isActive={item.id === selected?.conversationId}
              aria-current={
                item.id === selected?.conversationId ? "page" : undefined
              }
              disabled={busy}
              onClick={() => {
                void library.select({ kind: "conversation", id: item.id });
              }}
            >
              {runningConversationIds?.has(item.id) ? <Spinner aria-label="Conversa em execução" className="text-primary motion-reduce:animate-none" /> : <MessageSquare className={item.id === selected?.conversationId ? "text-primary" : "text-muted-foreground/55"} />}
              <span className="min-w-0 flex-1 truncate">{item.title}</span>
              {unreadConversationIds?.has(item.id) && <Badge role="img" aria-label="Mensagem não lida" title="Mensagem não lida" className="size-2 shrink-0 rounded-full border-0 bg-primary p-0 shadow-[0_0_6px_#61afef44]" />}
            </SidebarMenuButton>
          </LibraryItemMenu>
          <SidebarMenuAction showOnHover aria-label={`Excluir conversa ${item.title}`} title="Excluir conversa" disabled={busy} className="cursor-pointer text-muted-foreground hover:text-destructive focus-visible:opacity-100" onClick={() => { library.clearError(); setDeletion({ kind: "conversation", item }); }}><Trash2 aria-hidden="true" /></SidebarMenuAction>
        </SidebarMenuItem>}</SortableItem>
      ))}
    </SidebarMenu>
    </SortableList>
  );

  return (
    <aside
      aria-label="Workspace"
      className="h-full min-h-0 w-full overflow-hidden border-r border-border/70"
    >
      <SidebarProvider
        defaultOpen
        style={sidebarStyle}
        className="h-full min-h-0 w-full"
      >
        <Sidebar collapsible="none" className="h-full w-full bg-sidebar">
          <SidebarHeader className="gap-2 border-b border-border/70 px-3 pt-3 pb-3.5">
            <span className="micro-label flex items-center gap-2 px-1 text-muted-foreground"><Layers aria-hidden="true" className="size-3 text-onedark-cyan" />Workspace</span>
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
                  aria-label="Selecionar workspace"
                  className="h-8 w-full min-w-0 cursor-pointer border-transparent bg-card/30 text-xs shadow-none hover:border-border hover:bg-card/50"
                >
                  <SelectValue placeholder="Selecione um workspace" />
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
              <DropdownMenuTrigger render={<Button variant="ghost" size="icon" aria-label="Novo" title="Novo" className="shrink-0 cursor-pointer" disabled={busy || !snapshot} />}>
                <Plus />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-64">
                <DropdownMenuGroup>
                  <DropdownMenuItem className="cursor-pointer" disabled={!workspace || busy} onClick={() => { if (workspace) void library.addProject(workspace.id); }}><FolderPlus />Adicionar projeto</DropdownMenuItem>
                  <DropdownMenuItem className="cursor-pointer" disabled={busy} onClick={() => openDialog({ kind: "workspace" })}><Layers />Novo workspace</DropdownMenuItem>
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
              <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
                <div className="micro-label mb-2.5 flex items-center justify-between px-1.5 text-muted-foreground/80"><span>Projetos</span><span className="font-mono tabular-nums">{projects.length}</span></div>
                  {!workspace ? (
                    <Empty>
                      <EmptyHeader>
                        <EmptyTitle>Organize seus projetos</EmptyTitle>
                        <EmptyDescription>
                          Crie um workspace para começar.
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
                      <SortableList ids={projects.map(item => item.id)} onReorder={ids => saveOrder(projectOrderKey, ids)}><SidebarMenu aria-label="Projetos do workspace">
                        {projects.map((item) => {
                          const conversations = orderedItems(snapshot.conversations.filter(entry => entry.projectId === item.id)
                            .sort((a, b) => (b.lastActivityAt ?? b.createdAt) - (a.lastActivityAt ?? a.createdAt) || b.createdAt - a.createdAt || a.id.localeCompare(b.id)), layout.itemOrder[`chats:${item.id}`], entry => entry.id);
                          const visibleCount = visibleCounts[item.id] ?? 3;
                          const isCurrentProject = item.id === project?.id;
                          const isOpen = expanded[item.id] ?? item.id === project?.id;
                          return (
                          <SortableItem key={item.id} id={item.id} disabled={busy}>{sort => <SidebarMenuItem ref={sort.setNodeRef} style={sort.style} data-active-project={isCurrentProject ? "true" : undefined} className="rounded-r-md border-l-2 border-l-transparent py-0.5 transition-colors duration-150 data-[active-project=true]:border-l-onedark-cyan/70 data-[active-project=true]:bg-card/20 motion-reduce:transition-none">
                            <Collapsible role="group" aria-label={`Projeto ${item.name}`} aria-current={isCurrentProject ? "true" : undefined} open={isOpen} onOpenChange={(open) => setExpanded(values => ({ ...values, [item.id]: open }))}>
                            <LibraryItemMenu
                              disabled={busy}
                              onEdit={() =>
                                openDialog({ kind: "project", item })
                              }
                              onDelete={() => { library.clearError(); setDeletion({ kind: "project", item }); }}
                              onMove={() => { library.clearError(); setMoving(item); }}
                            >
                              <CollapsibleTrigger render={<SidebarMenuButton
                                ref={sort.setActivatorNodeRef}
                                {...sort.listeners}
                                aria-describedby={sort.attributes["aria-describedby"]}
                                size="lg"
                                className={`h-8 cursor-pointer text-muted-foreground data-active:bg-transparent! data-active:shadow-none! ${isCurrentProject ? "font-semibold text-foreground!" : ""}`}
                                isActive={item.id === project?.id}
                                disabled={busy}
                                title={item.path}
                                onClick={() => {
                                  if (item.id !== project?.id) void library.select({
                                    kind: "project",
                                    id: item.id,
                                  });
                                }}
                              />}>
                                <ChevronRight aria-hidden="true" className={`size-3 text-muted-foreground/55 transition-transform motion-reduce:transition-none ${isOpen ? "rotate-90" : ""}`} />
                                {snapshot.conversations.some(entry => entry.projectId === item.id && runningConversationIds?.has(entry.id)) ? <Spinner aria-label="Projeto com conversa em execução" className="text-primary motion-reduce:animate-none" /> : <Folder className={isCurrentProject ? "text-onedark-cyan" : "text-muted-foreground/55"} />}
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate">
                                    {item.name}
                                  </span>
                                </span>
                                {conversations.some(entry => unreadConversationIds?.has(entry.id)) && <Badge role="img" aria-label="Projeto com mensagens não lidas" title="Mensagens não lidas" className="mr-1 size-2 shrink-0 rounded-full border-0 bg-primary p-0" />}
                              </CollapsibleTrigger>
                            </LibraryItemMenu>
                              <SidebarMenuAction
                                type="button"
                                aria-label={`Nova conversa em ${item.name}`}
                                title="Nova conversa"
                                className="cursor-pointer text-muted-foreground peer-data-[size=lg]/menu-button:top-2 disabled:pointer-events-none disabled:opacity-50"
                                disabled={busy}
                                onClick={() => {
                                  setExpanded(values => ({ ...values, [item.id]: true }));
                                  void library.createConversation(item.id);
                                }}
                              >
                                <Plus aria-hidden="true" />
                              </SidebarMenuAction>
                              <CollapsibleContent className="my-0.5 ml-2.5 border-l border-border/60 pl-1.5">
                                <SidebarMenu className="mb-1">
                                  <SidebarMenuItem>
                                    <SidebarMenuButton className="h-7 cursor-pointer text-muted-foreground data-active:text-foreground!" disabled={busy}
                                      isActive={selected?.projectId === item.id && !selected.conversationId}
                                      aria-current={selected?.projectId === item.id && !selected.conversationId ? "page" : undefined}
                                      onClick={() => { void library.select({ kind: "project", id: item.id }); }}>
                                      <LayoutDashboard className={selected?.projectId === item.id && !selected.conversationId ? "text-onedark-cyan" : "text-muted-foreground/55"} /><span>Dashboard</span>
                                    </SidebarMenuButton>
                                  </SidebarMenuItem>
                                </SidebarMenu>
                                {conversationList(conversations.slice(0, visibleCount), conversations, item.id)}
                                {conversations.length > visibleCount && <Button variant="ghost" size="sm" className="mt-0.5 h-7 w-full cursor-pointer justify-start pl-8 text-xs text-muted-foreground/80" onClick={() => setVisibleCounts(counts => ({ ...counts, [item.id]: visibleCount + 10 }))}>Ver mais<span className="ml-auto font-mono text-[10px]">+{Math.min(10, conversations.length - visibleCount)}</span></Button>}
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
              : dialog.kind === "project"
                ? dialog.item.name
                : dialog.item.title
          }
          projectPath={dialog.kind === "project" ? dialog.item.path : undefined}
          pending={pending}
          error={error}
          onClose={() => {
            setDialog(null);
            library.clearError();
          }}
          onSubmit={(name) =>
            dialog.kind === "workspace"
              ? library.createWorkspace(name)
              : dialog.kind === "project"
                ? library.renameProject(dialog.item.id, name)
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
      {moving && <MoveProjectDialog project={moving} workspaces={snapshot?.workspaces ?? []} library={library} onClose={() => { setMoving(null); library.clearError(); }} />}
    </aside>
  );
}
