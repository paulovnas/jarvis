import { useState, type CSSProperties } from "react";
import {
  Folder,
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
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { Conversation, Project } from "@/core/library";
import type { LibraryController } from "@/hooks/use-library";
import { ItemNameDialog } from "./ItemNameDialog";
import { LibraryItemMenu } from "./LibraryItemMenu";
import { DeleteItemDialog } from "./DeleteItemDialog";

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
  const conversations =
    snapshot?.conversations.filter((item) => item.projectId === project?.id) ??
    [];
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
          <SidebarHeader className="gap-3 border-b border-border p-3">
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
                  className="w-full min-w-0 cursor-pointer"
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
              <Button
                variant="ghost"
                size="icon"
                className="shrink-0 cursor-pointer"
                aria-label="Novo workspace"
                disabled={busy || !snapshot}
                onClick={() => openDialog({ kind: "workspace" })}
              >
                <Plus />
              </Button>
            </div>
            <Button
              className="w-full cursor-pointer"
              disabled={busy || !project}
              onClick={() => {
                if (project) void library.createConversation(project.id);
              }}
            >
              <MessageSquarePlus />
              Nova conversa
            </Button>
          </SidebarHeader>
          <SidebarContent className="gap-0">
            {loading && (
              <div role="status" className="space-y-3 p-4">
                <span className="sr-only">Carregando projetos</span>
                <Skeleton className="h-12 w-full" />
                <Skeleton className="h-12 w-full" />
              </div>
            )}
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
              <Tabs defaultValue="projects" className="min-h-0 flex-1 gap-0">
                <TabsList className="m-3 grid w-auto grid-cols-2">
                  <TabsTrigger value="projects" className="cursor-pointer">
                    Projetos
                  </TabsTrigger>
                  <TabsTrigger value="conversations" className="cursor-pointer">
                    Conversas
                  </TabsTrigger>
                </TabsList>
                <TabsContent
                  value="projects"
                  className="m-0 overflow-y-auto px-3 pb-3"
                >
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
                      <Button
                        variant="outline"
                        className="mb-3 w-full cursor-pointer"
                        disabled={busy}
                        onClick={() => {
                          void library.addProject(workspace.id);
                        }}
                      >
                        <FolderPlus />
                        Novo projeto
                      </Button>
                      {!projects.length && (
                        <Empty>
                          <EmptyHeader>
                            <EmptyTitle>Nenhum projeto</EmptyTitle>
                            <EmptyDescription>
                              Selecione a pasta de um projeto neste workspace.
                            </EmptyDescription>
                          </EmptyHeader>
                        </Empty>
                      )}
                      <SidebarMenu aria-label="Projetos do workspace">
                        {projects.map((item) => (
                          <SidebarMenuItem key={item.id}>
                            <LibraryItemMenu
                              disabled={busy}
                              onEdit={() =>
                                openDialog({ kind: "project", item })
                              }
                              onDelete={() => { library.clearError(); setDeletion({ kind: "project", item }); }}
                            >
                              <SidebarMenuButton
                                size="lg"
                                className="h-auto min-h-12 cursor-pointer"
                                isActive={item.id === project?.id}
                                disabled={busy}
                                onClick={() => {
                                  void library.select({
                                    kind: "project",
                                    id: item.id,
                                  });
                                }}
                              >
                                {snapshot.conversations.some(entry => entry.projectId === item.id && runningConversationIds?.has(entry.id)) ? <Spinner aria-label="Projeto com conversa em execução" className="text-primary motion-reduce:animate-none" /> : <Folder className="text-primary" />}
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate">
                                    {item.name}
                                  </span>
                                  <span
                                    className="block truncate text-xs text-muted-foreground"
                                    title={item.path}
                                  >
                                    {item.path}
                                  </span>
                                </span>
                                <Badge variant="secondary">
                                  {
                                    snapshot.conversations.filter(
                                      (entry) => entry.projectId === item.id,
                                    ).length
                                  }
                                </Badge>
                              </SidebarMenuButton>
                            </LibraryItemMenu>
                            {item.id === project?.id && (
                              <div className="my-2 ml-4 border-l border-border pl-2">
                                {conversations.length ? (
                                  conversationList(conversations)
                                ) : (
                                  <p className="p-2 text-xs text-muted-foreground">
                                    Nenhuma conversa. Crie a primeira acima.
                                  </p>
                                )}
                              </div>
                            )}
                          </SidebarMenuItem>
                        ))}
                      </SidebarMenu>
                    </>
                  )}
                </TabsContent>
                <TabsContent
                  value="conversations"
                  className="m-0 overflow-y-auto px-3 pb-3"
                >
                  {project && (
                    <p className="mb-3 truncate text-xs text-muted-foreground">
                      {project.name}
                    </p>
                  )}
                  {conversations.length ? (
                    conversationList(conversations)
                  ) : (
                    <Empty>
                      <EmptyHeader>
                        <EmptyTitle>
                          {project
                            ? "Nenhuma conversa"
                            : "Selecione um projeto"}
                        </EmptyTitle>
                        <EmptyDescription>
                          {project
                            ? "Crie a primeira conversa usando o botão acima."
                            : "As conversas pertencem ao projeto selecionado."}
                        </EmptyDescription>
                      </EmptyHeader>
                    </Empty>
                  )}
                </TabsContent>
              </Tabs>
            )}
          </SidebarContent>
          <SidebarFooter className="border-t border-border p-3">
            <Button
              variant="ghost"
              className="w-full cursor-pointer justify-start"
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
