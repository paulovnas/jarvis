import { useState, type CSSProperties } from "react";
import {
  Briefcase,
  Circle,
  Folder,
  FolderPlus,
  MessageSquare,
  MessageSquarePlus,
  Plus,
  Settings,
} from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Sidebar as SidebarPrimitive,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuItem,
  SidebarProvider,
} from "@/components/ui/sidebar";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

const WORKSPACES = ["Pessoal", "Trabalho"] as const;
type Workspace = (typeof WORKSPACES)[number];
type HomeTab = "projects" | "conversations";

export interface ConversationItem {
  id: string;
  title: string;
  active?: boolean;
  time?: string;
}

export interface ProjectData {
  id: string;
  name: string;
  description: string;
  conversations: ConversationItem[];
}

const INITIAL_PROJECTS: ProjectData[] = [
  {
    id: "jarvis",
    name: "Jarvis",
    description: "coding-agent desktop",
    conversations: [
      { id: "c1", title: "Onboarding app shell", active: true, time: "Hoje" },
      { id: "c2", title: "Persistência nativa", active: false, time: "Ontem" },
      { id: "c3", title: "Revisão de componentes", active: false, time: "3d atrás" },
    ],
  },
  {
    id: "metis",
    name: "Metis",
    description: "base de referência",
    conversations: [
      { id: "c4", title: "Exploração de TUI", active: false, time: "1sem atrás" },
      { id: "c5", title: "Mapeamento de ferramentas", active: false, time: "2sem atrás" },
    ],
  },
];

const sidebarStyle = {
  "--sidebar-width": "100%",
  "--sidebar-width-icon": "100%",
  height: "100%",
  minHeight: 0,
} as CSSProperties;

export function AppSidebar() {
  const [workspace, setWorkspace] = useState<Workspace>("Pessoal");
  const [activeTab, setActiveTab] = useState<HomeTab>("projects");
  const [activeProjectId, setActiveProjectId] = useState<string>("jarvis");
  const [activeConversationId, setActiveConversationId] = useState<string>("c1");
  const [projects] = useState<ProjectData[]>(INITIAL_PROJECTS);

  const activeProject =
    projects.find((p) => p.id === activeProjectId) ?? projects[0];

  const handleNewWorkspace = () => {
    toast.info("Novo Workspace (mock)", {
      description: "Permite criar e alternar entre múltiplos ambientes de trabalho.",
    });
  };

  const handleNewProject = () => {
    toast.info("Novo Projeto (mock)", {
      description: "Permite abrir uma nova pasta ou repositório local.",
    });
  };

  const handleNewConversation = () => {
    toast.success("Nova conversa iniciada!", {
      description: `Sessão aberta no projeto ativo: ${activeProject.name}.`,
    });
  };

  return (
    <aside
      aria-label="Workspace"
      className="h-full min-h-0 w-full overflow-hidden border-r border-[#3e4451]"
    >
      <SidebarProvider
        defaultOpen
        style={sidebarStyle}
        className="h-full min-h-0 w-full"
      >
        <SidebarPrimitive
          side="left"
          variant="sidebar"
          collapsible="none"
          style={{ width: "100%", height: "100%" }}
          className="h-full w-full border-0 bg-[#1e2227] text-[#abb2bf]"
        >
          {/* Header com Seletor de Workspace e Ações Rápidas */}
          <SidebarHeader className="gap-2.5 border-b border-[#3e4451] p-3">
            <div className="flex items-center justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2">
                <span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-[#56b6c2]/15 text-[#56b6c2]">
                  <Briefcase aria-hidden="true" className="size-4" />
                </span>
                <div className="min-w-0">
                  <p className="truncate text-sm font-semibold text-[#e6e6e6]">
                    Workspace
                  </p>
                  <p className="text-[11px] text-[#7f848e]">Ambiente local</p>
                </div>
              </div>
              <Badge
                variant="outline"
                className="shrink-0 border-[#56b6c2]/40 text-[10px] text-[#56b6c2]"
              >
                local
              </Badge>
            </div>

            {/* Linha do Seletor de Workspace com Botão de Adição */}
            <div className="flex items-center gap-1.5">
              <Select
                value={workspace}
                onValueChange={(value) => {
                  if (WORKSPACES.includes(value as Workspace)) {
                    setWorkspace(value as Workspace);
                  }
                }}
              >
                <SelectTrigger
                  aria-label="Workspace"
                  className="h-8.5 w-full cursor-pointer rounded-lg border-[#3e4451] bg-[#282c34] px-3 text-xs font-medium text-[#e6e6e6] shadow-xs transition-colors hover:border-[#61afef]/40 hover:bg-[#2c313a]"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent className="border-[#3e4451] bg-[#21252b] text-[#e6e6e6]">
                  {WORKSPACES.map((option) => (
                    <SelectItem
                      key={option}
                      value={option}
                      className="cursor-pointer py-2 text-xs"
                    >
                      {option}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Button
                type="button"
                variant="outline"
                size="icon"
                onClick={handleNewWorkspace}
                aria-label="Novo Workspace"
                title="Novo Workspace"
                className="size-8.5 shrink-0 cursor-pointer rounded-lg border-[#3e4451] bg-[#282c34] text-[#abb2bf] hover:border-[#56b6c2]/40 hover:bg-[#2c313a] hover:text-[#e6e6e6]"
              >
                <Plus className="size-4" />
              </Button>
            </div>

            {/* Botão de Ação Primária: Nova Conversa */}
            <Button
              type="button"
              onClick={handleNewConversation}
              aria-label="Nova conversa"
              className="mt-0.5 h-8.5 w-full cursor-pointer justify-center gap-2 rounded-lg border border-[#61afef]/30 bg-[#61afef]/15 text-xs font-medium text-[#61afef] shadow-xs transition-all hover:border-[#61afef]/50 hover:bg-[#61afef]/25 active:scale-[0.99]"
            >
              <MessageSquarePlus className="size-4" />
              <span>Nova conversa</span>
            </Button>
          </SidebarHeader>

          {/* Conteúdo com Abas de Navegação */}
          <SidebarContent className="min-h-0 overflow-hidden p-2">
            <Tabs
              value={activeTab}
              onValueChange={(val) => {
                if (val) setActiveTab(val as HomeTab);
              }}
              className="flex min-h-0 flex-1 flex-col"
            >
              <TabsList
                variant="line"
                className="grid h-9 w-full grid-cols-2 rounded-none border-b border-[#3e4451] p-0"
              >
                <TabsTrigger
                  value="projects"
                  onClick={() => setActiveTab("projects")}
                  className="cursor-pointer text-xs data-active:text-[#61afef]"
                >
                  <Folder aria-hidden="true" className="size-3.5 mr-1.5" />
                  Projetos
                </TabsTrigger>
                <TabsTrigger
                  value="conversations"
                  onClick={() => setActiveTab("conversations")}
                  className="cursor-pointer text-xs data-active:text-[#61afef]"
                >
                  <MessageSquare aria-hidden="true" className="size-3.5 mr-1.5" />
                  Conversas
                </TabsTrigger>
              </TabsList>
              {activeTab === "projects" ? (
              <TabsContent
                value="projects"
                className="min-h-0 flex-1 overflow-hidden pt-2"
              >
                <ScrollArea className="h-full min-h-0 pr-1">
                  <SidebarGroup className="p-0">
                    <div className="mb-2 flex items-center justify-between px-2 text-xs">
                      <span className="text-[10px] font-semibold uppercase tracking-wider text-[#7f848e]">
                        Projetos
                      </span>
                      <Button
                        type="button"
                        variant="ghost"
                        size="xs"
                        onClick={handleNewProject}
                        aria-label="Novo projeto"
                        title="Novo projeto"
                        className="h-6 cursor-pointer gap-1 px-1.5 text-[11px] text-[#7f848e] hover:bg-[#2c313a] hover:text-[#e6e6e6]"
                      >
                        <FolderPlus className="size-3 text-[#61afef]" />
                        <span>Novo</span>
                      </Button>
                    </div>

                    <SidebarGroupContent>
                      <SidebarMenu className="gap-1.5">
                        {projects.map((project) => {
                          const isSelected = project.id === activeProjectId;
                          return (
                            <SidebarMenuItem key={project.id}>
                              <div
                                role="button"
                                tabIndex={0}
                                aria-current={isSelected ? "page" : undefined}
                                onClick={() => setActiveProjectId(project.id)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter" || e.key === " ") {
                                    setActiveProjectId(project.id);
                                  }
                                }}
                                className={`flex cursor-pointer flex-col rounded-lg border p-2.5 transition-all ${
                                  isSelected
                                    ? "border-[#61afef]/40 bg-[#61afef]/10 shadow-xs"
                                    : "border-transparent bg-transparent hover:border-[#3e4451]/60 hover:bg-[#2c313a]/40"
                                }`}
                              >
                                <div className="flex items-center justify-between gap-2">
                                  <div className="flex min-w-0 items-center gap-2">
                                    <Folder
                                      aria-hidden="true"
                                      className={`size-4 shrink-0 ${
                                        isSelected
                                          ? "text-[#61afef]"
                                          : "text-[#7f848e]"
                                      }`}
                                    />
                                    <span
                                      className={`truncate text-sm ${
                                        isSelected
                                          ? "font-semibold text-[#e6e6e6]"
                                          : "text-[#abb2bf]"
                                      }`}
                                    >
                                      {project.name}
                                    </span>
                                  </div>

                                  <div className="flex items-center gap-1.5">
                                    {isSelected && (
                                      <Badge className="h-4.5 border-0 bg-[#61afef]/20 px-1.5 text-[9px] font-medium text-[#61afef]">
                                        Projeto ativo
                                      </Badge>
                                    )}
                                    <Badge
                                      variant="outline"
                                      className="h-4.5 border-[#3e4451] px-1 font-mono text-[9px] text-[#7f848e]"
                                    >
                                      {project.conversations.length}
                                    </Badge>
                                  </div>
                                </div>

                                <p className="mt-1 truncate text-[11px] text-[#7f848e]">
                                  {project.description}
                                </p>

                                {/* Conversas aninhadas exibidas diretamente ao selecionar o projeto */}
                                {isSelected && (
                                  <div className="mt-2.5 space-y-1 border-t border-[#61afef]/20 pt-2">
                                    {project.conversations.map((conv) => (
                                      <div
                                        key={conv.id}
                                        role="button"
                                        tabIndex={0}
                                        onClick={(e) => {
                                          e.stopPropagation();
                                          setActiveConversationId(conv.id);
                                        }}
                                        onKeyDown={(e) => {
                                          if (e.key === "Enter" || e.key === " ") {
                                            e.stopPropagation();
                                            setActiveConversationId(conv.id);
                                          }
                                        }}
                                        className={`flex cursor-pointer items-center justify-between gap-2 rounded-md px-2 py-1.5 text-[11px] transition-colors ${
                                          conv.id === activeConversationId
                                            ? "bg-[#282c34] font-medium text-[#e6e6e6]"
                                            : "text-[#abb2bf] hover:bg-[#2c313a]/60 hover:text-[#e6e6e6]"
                                        }`}
                                      >
                                        <div className="flex min-w-0 items-center gap-1.5">
                                          <MessageSquare className="size-3 shrink-0 text-[#56b6c2]" />
                                          <span className="truncate">
                                            {conv.title}
                                          </span>
                                        </div>
                                        {conv.id === activeConversationId && (
                                          <Circle className="size-1.5 shrink-0 fill-[#98c379] text-[#98c379]" />
                                        )}
                                      </div>
                                    ))}
                                  </div>
                                )}
                              </div>
                            </SidebarMenuItem>
                          );
                        })}
                      </SidebarMenu>
                    </SidebarGroupContent>
                  </SidebarGroup>
                </ScrollArea>
              </TabsContent>
              ) : (
                <TabsContent
                value="conversations"
                className="min-h-0 flex-1 overflow-hidden pt-2"
              >
                <ScrollArea className="h-full min-h-0 pr-1">
                  <SidebarGroup className="p-0">
                    {/* Header contextual: informa o projeto pai das conversas */}
                    <div className="mb-2 flex items-center justify-between rounded-lg border border-[#3e4451]/60 bg-[#21252b]/80 p-2 text-xs">
                      <div className="min-w-0">
                        <span className="block text-[10px] uppercase tracking-wider text-[#7f848e]">
                          Conversas do projeto
                        </span>
                        <div className="flex items-center gap-1.5">
                          <Folder className="size-3.5 text-[#61afef]" />
                          <span className="truncate font-semibold text-[#e6e6e6]">
                            {activeProject.name}
                          </span>
                        </div>
                      </div>

                      <Button
                        type="button"
                        variant="ghost"
                        size="xs"
                        onClick={handleNewConversation}
                        aria-label="Nova conversa no projeto"
                        title="Nova conversa no projeto"
                        className="h-6 cursor-pointer gap-1 px-2 text-[10px] text-[#61afef] hover:bg-[#61afef]/10"
                      >
                        <Plus className="size-3" />
                        <span>Nova</span>
                      </Button>
                    </div>

                    <SidebarGroupContent>
                      <SidebarMenu className="gap-1">
                        {activeProject.conversations.map((conversation) => {
                          const isSelected =
                            conversation.id === activeConversationId;
                          return (
                            <SidebarMenuItem key={conversation.id}>
                              <div
                                role="button"
                                tabIndex={0}
                                onClick={() =>
                                  setActiveConversationId(conversation.id)
                                }
                                onKeyDown={(e) => {
                                  if (e.key === "Enter" || e.key === " ") {
                                    setActiveConversationId(conversation.id);
                                  }
                                }}
                                className={`flex cursor-pointer items-center justify-between gap-2 rounded-lg px-2.5 py-2 transition-all ${
                                  isSelected
                                    ? "bg-[#2c313a] text-[#e6e6e6] shadow-xs"
                                    : "text-[#abb2bf] hover:bg-[#21252b] hover:text-[#e6e6e6]"
                                }`}
                              >
                                <div className="flex min-w-0 items-center gap-2">
                                  <MessageSquare
                                    aria-hidden="true"
                                    className="size-3.5 shrink-0 text-[#56b6c2]"
                                  />
                                  <span className="truncate text-xs font-medium">
                                    {conversation.title}
                                  </span>
                                </div>
                                {isSelected && (
                                  <Circle
                                    aria-hidden="true"
                                    className="size-1.5 shrink-0 fill-[#98c379] text-[#98c379]"
                                  />
                                )}
                              </div>
                            </SidebarMenuItem>
                          );
                        })}
                      </SidebarMenu>
                    </SidebarGroupContent>
                  </SidebarGroup>
                </ScrollArea>
              </TabsContent>
              )}
            </Tabs>
          </SidebarContent>

          {/* Footer com Configurações */}
          <SidebarFooter className="border-t border-[#3e4451] p-3">
            <Button
              type="button"
              variant="ghost"
              onClick={() => toast.info("Em breve")}
              className="w-full cursor-pointer justify-start gap-2 text-[#abb2bf] hover:bg-[#2c313a] hover:text-[#e6e6e6]"
            >
              <Settings aria-hidden="true" className="size-4 text-[#7f848e]" />
              <span>Configurações</span>
            </Button>
          </SidebarFooter>
        </SidebarPrimitive>
      </SidebarProvider>
    </aside>
  );
}
