import { useState, type CSSProperties } from "react";
import {
  Folder,
  ChevronRight,
  Layers,
  FolderPlus,
  MessageSquare,
  MessageSquarePlus,
  Plus,
  Settings,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
} from "@/components/ui/sidebar";
import { SidebarSkeleton } from "./LoadingSkeletons";
import { Spinner } from "@/components/ui/spinner";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import type { Conversation, Project } from "@/core/library";
import type { LibraryController } from "@/hooks/use-library";
import { ItemNameDialog } from "./ItemNameDialog";
import { LibraryItemMenu } from "./LibraryItemMenu";
import { DeleteItemDialog } from "./DeleteItemDialog";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";

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
  onOpenSettings,
  runningConversationIds,
}: {
  library: LibraryController;
  onOpenSettings: () => void;
  runningConversationIds?: ReadonlySet<string>;
}) {
  const [dialog, setDialog] = useState<NameDialog | null>(null);
  const { layout, updateLayout } = useDesktopLayout();
  const expanded = layout.expandedProjects;
  const setExpanded = (update: (values: Record<string, boolean>) => Record<string, boolean>) => updateLayout(current => ({ expandedProjects: update(current.expandedProjects) }));
  const [deletion, setDeletion] = useState<Exclude<NameDialog, { kind: "workspace" }> | null>(null);
  const { snapshot, loading, pending, error } = library;
  const selected = snapshot?.selection;
  const workspace = snapshot?.workspaces.find(
    (item) => item.id === selected?.workspaceId,
  );
  const projects =
    snapshot?.projects.filter((item) => item.workspaceId === workspace?.id) ??
    [];
  const project = projects.find((item) => item.id === selected?.projectId);
  const busy = loading || pending;
  const openDialog = (value: NameDialog) => {
    library.clearError();
    setDialog(value);
  };
  const conversationList = (items: Conversation[]) => (
    <SidebarMenu aria-label="Conversas do projeto">
      {items.map((item) => (
        <SidebarMenuItem key={item.id}>
          <LibraryItemMenu
            disabled={busy}
            onEdit={() => openDialog({ kind: "conversation", item })}
            onDelete={() => { library.clearError(); setDeletion({ kind: "conversation", item }); }}
          >
            <SidebarMenuButton
              className="cursor-pointer"
              isActive={item.id === selected?.conversationId}
              aria-current={
                item.id === selected?.conversationId ? "page" : undefined
              }
              disabled={busy}
              onClick={() => {
                void library.select({ kind: "conversation", id: item.id });
              }}
            >
              {runningConversationIds?.has(item.id) ? <Spinner aria-label="Conversa em execução" className="text-primary motion-reduce:animate-none" /> : <MessageSquare />}
              <span className="truncate">{item.title}</span>
            </SidebarMenuButton>
          </LibraryItemMenu>
        </SidebarMenuItem>
      ))}
    </SidebarMenu>
  );

  return (
    <aside
      aria-label="Workspace"
      className="h-full min-h-0 w-full overflow-hidden border-r border-border"
    >
      <SidebarProvider
        defaultOpen
        style={sidebarStyle}
        className="h-full min-h-0 w-full"
      >
        <Sidebar collapsible="none" className="h-full w-full bg-sidebar">
          <SidebarHeader className="gap-2 border-b border-border px-3 pt-3 pb-3.5">
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
                  className="h-8 w-full min-w-0 cursor-pointer border-border bg-card/60 text-xs shadow-[inset_0_1px_0_#ffffff0a]"
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
                  <DropdownMenuLabel className="truncate">{project ? project.name : "Selecione um projeto"}</DropdownMenuLabel>
                  <DropdownMenuItem className="cursor-pointer" disabled={!project || busy} onClick={() => { if (project) { setExpanded(values => ({ ...values, [project.id]: true })); void library.createConversation(project.id); } }}><MessageSquarePlus />Nova conversa</DropdownMenuItem>
                </DropdownMenuGroup>
                <DropdownMenuSeparator />
                <DropdownMenuGroup>
                  <DropdownMenuItem className="cursor-pointer" disabled={!workspace || busy} onClick={() => { if (workspace) void library.addProject(workspace.id); }}><FolderPlus />Novo projeto</DropdownMenuItem>
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
              <div className="min-h-0 flex-1 overflow-y-auto p-3">
                <div className="micro-label mb-3 flex items-center justify-between px-1 text-muted-foreground"><span>Projetos</span><span className="font-mono tabular-nums">{projects.length}</span></div>
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
                              Use Novo → Novo projeto para selecionar uma pasta.
                            </EmptyDescription>
                          </EmptyHeader>
                        </Empty>
                      )}
                      <SidebarMenu aria-label="Projetos do workspace">
                        {projects.map((item) => {
                          const conversations = snapshot.conversations.filter(entry => entry.projectId === item.id);
                          const isOpen = expanded[item.id] ?? item.id === project?.id;
                          return (
                          <SidebarMenuItem key={item.id}>
                            <Collapsible open={isOpen} onOpenChange={(open) => setExpanded(values => ({ ...values, [item.id]: open }))}>
                            <LibraryItemMenu
                              disabled={busy}
                              onEdit={() =>
                                openDialog({ kind: "project", item })
                              }
                              onDelete={() => { library.clearError(); setDeletion({ kind: "project", item }); }}
                            >
                              <CollapsibleTrigger render={<SidebarMenuButton
                                size="lg"
                                className="h-9 cursor-pointer"
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
                                <ChevronRight aria-hidden="true" className={`size-3 text-muted-foreground transition-transform motion-reduce:transition-none ${isOpen ? "rotate-90" : ""}`} />
                                {snapshot.conversations.some(entry => entry.projectId === item.id && runningConversationIds?.has(entry.id)) ? <Spinner aria-label="Projeto com conversa em execução" className="text-primary motion-reduce:animate-none" /> : <Folder className="text-primary" />}
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate">
                                    {item.name}
                                  </span>
                                </span>
                                <Badge variant="secondary" className="border border-border bg-transparent font-mono text-muted-foreground">
                                  {conversations.length}
                                </Badge>
                              </CollapsibleTrigger>
                            </LibraryItemMenu>
                              <CollapsibleContent className="my-1 ml-3 border-l border-border pl-2">
                                {conversations.length ? (
                                  conversationList(conversations)
                                ) : (
                                  <p className="p-2 text-xs text-muted-foreground">
                                    Use Novo → Nova conversa para começar.
                                  </p>
                                )}
                              </CollapsibleContent>
                            </Collapsible>
                          </SidebarMenuItem>
                        ); })}
                      </SidebarMenu>
                    </>
                  )}
              </div>
            )}
          </SidebarContent>
          <SidebarFooter className="border-t border-border px-3 py-2">
            <Button
              variant="ghost"
              className="h-9 w-full cursor-pointer justify-start text-xs text-muted-foreground"
              onClick={onOpenSettings}
            >
              <Settings />
              Configurações
            </Button>
          </SidebarFooter>
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
    </aside>
  );
}
