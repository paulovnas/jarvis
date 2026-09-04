import { useState } from "react";
import {
  Bot,
  CheckCircle2,
  ChevronDown,
  ChevronsDownUp,
  ChevronsUpDown,
  Circle,
  FileCode2,
  ListChecks,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";

export interface ChangedFileItem {
  path: string;
  diffStats: { added: number; removed: number };
}

export interface PlanItem {
  id: string;
  label: string;
  completed: boolean;
}

export interface SubagentItem {
  name: string;
  role: string;
  status: "idle" | "running" | "completed";
}

const DEFAULT_FILES: ChangedFileItem[] = [
  {
    path: "src/components/chat/ChatArea.tsx",
    diffStats: { added: 148, removed: 12 },
  },
  {
    path: "src/components/chat/ChatComposer.tsx",
    diffStats: { added: 84, removed: 22 },
  },
  {
    path: "src/components/layout/Home.tsx",
    diffStats: { added: 36, removed: 78 },
  },
  {
    path: "src/App.tsx",
    diffStats: { added: 8, removed: 4 },
  },
];

const DEFAULT_PLAN: PlanItem[] = [
  {
    id: "p1",
    label: "Persistência SQLite em ~/.jarvis/jarvis.db",
    completed: true,
  },
  {
    id: "p2",
    label: "Shell desktop de três colunas redimensionáveis",
    completed: true,
  },
  {
    id: "p3",
    label: "Interface do Chat com suporte a ferramentas e raciocínio",
    completed: true,
  },
  {
    id: "p4",
    label: "Integração do loop de execução em tempo real",
    completed: false,
  },
];

const DEFAULT_SUBAGENTS: SubagentItem[] = [
  {
    name: "Full Construtor",
    role: "Implementação e testes",
    status: "completed",
  },
  {
    name: "Direct Designer",
    role: "Refinamento e layout UI/UX",
    status: "running",
  },
  {
    name: "Revisor",
    role: "Auditoria e quality gates",
    status: "idle",
  },
];

export function Inspector() {
  const [filesOpen, setFilesOpen] = useState(true);
  const [planOpen, setPlanOpen] = useState(true);
  const [subagentsOpen, setSubagentsOpen] = useState(true);

  const allOpen = filesOpen && planOpen && subagentsOpen;

  const toggleAll = () => {
    if (allOpen) {
      setFilesOpen(false);
      setPlanOpen(false);
      setSubagentsOpen(false);
    } else {
      setFilesOpen(true);
      setPlanOpen(true);
      setSubagentsOpen(true);
    }
  };

  return (
    <aside
      aria-label="Inspector"
      className="flex h-full min-h-0 w-full flex-col overflow-hidden bg-[#21252b]"
    >
      {/* Header do Inspector */}
      <header className="flex h-12 shrink-0 items-center justify-between border-b border-[#3e4451] px-4">
        <div>
          <p className="text-sm font-semibold text-[#e6e6e6]">Inspector</p>
          <p className="text-[11px] text-[#7f848e]">Contexto da sessão</p>
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            type="button"
            variant="ghost"
            size="xs"
            onClick={toggleAll}
            aria-label={allOpen ? "Recolher todos os cards" : "Expandir todos os cards"}
            className="h-6 cursor-pointer gap-1 px-2 text-[10px] text-[#abb2bf] hover:bg-[#2c313a] hover:text-[#e6e6e6]"
          >
            {allOpen ? (
              <>
                <ChevronsDownUp className="size-3 text-[#7f848e]" />
                <span className="hidden sm:inline">Recolher</span>
              </>
            ) : (
              <>
                <ChevronsUpDown className="size-3 text-[#7f848e]" />
                <span className="hidden sm:inline">Expandir</span>
              </>
            )}
          </Button>
          <Badge
            variant="outline"
            className="border-[#3e4451] text-[10px] text-[#7f848e]"
          >
            mock
          </Badge>
        </div>
      </header>

      {/* Lista de seções colapsáveis com ScrollArea */}
      <ScrollArea className="min-h-0 flex-1">
        <div className="space-y-2.5 p-3">
          {/* Card 1: Arquivos alterados */}
          <section
            aria-labelledby="inspector-files-title"
            className="overflow-hidden rounded-xl border border-[#3e4451]/80 bg-[#1e2227]/60 shadow-xs transition-colors hover:border-[#3e4451]"
          >
            <button
              type="button"
              aria-expanded={filesOpen}
              onClick={() => setFilesOpen((prev) => !prev)}
              className="flex w-full cursor-pointer items-center justify-between gap-2 px-3 py-2.5 text-left transition-colors hover:bg-[#2c313a]/50"
            >
              <div className="flex min-w-0 items-center gap-2">
                <span className="flex size-6 items-center justify-center rounded-md bg-[#61afef]/15 text-[#61afef]">
                  <FileCode2 className="size-3.5" />
                </span>
                <span
                  id="inspector-files-title"
                  className="truncate text-xs font-semibold text-[#e6e6e6]"
                >
                  Arquivos alterados
                </span>
              </div>
              <div className="flex items-center gap-2">
                <Badge
                  variant="outline"
                  className="h-5 border-[#3e4451] px-1.5 font-mono text-[10px] text-[#7f848e]"
                >
                  {DEFAULT_FILES.length}
                </Badge>
                <ChevronDown
                  className={`size-3.5 text-[#7f848e] transition-transform duration-200 ${
                    filesOpen ? "" : "-rotate-90"
                  }`}
                />
              </div>
            </button>

            {filesOpen && (
              <div className="space-y-1 border-t border-[#3e4451]/50 bg-[#181a1f]/70 p-2 text-xs">
                {DEFAULT_FILES.map((file) => (
                  <div
                    key={file.path}
                    className="flex items-center justify-between gap-2 rounded-md bg-[#21252b]/70 px-2.5 py-1.5 text-[11px] text-[#abb2bf] transition-colors hover:bg-[#2c313a]/50 hover:text-[#e6e6e6]"
                  >
                    <span className="min-w-0 flex-1 truncate font-mono">
                      {file.path}
                    </span>
                    <span className="flex shrink-0 items-center gap-1 font-mono text-[10px]">
                      <span className="text-[#98c379]">+{file.diffStats.added}</span>
                      <span className="text-[#e06c75]">-{file.diffStats.removed}</span>
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>

          {/* Card 2: Plano */}
          <section
            aria-labelledby="inspector-plan-title"
            className="overflow-hidden rounded-xl border border-[#3e4451]/80 bg-[#1e2227]/60 shadow-xs transition-colors hover:border-[#3e4451]"
          >
            <button
              type="button"
              aria-expanded={planOpen}
              onClick={() => setPlanOpen((prev) => !prev)}
              className="flex w-full cursor-pointer items-center justify-between gap-2 px-3 py-2.5 text-left transition-colors hover:bg-[#2c313a]/50"
            >
              <div className="flex min-w-0 items-center gap-2">
                <span className="flex size-6 items-center justify-center rounded-md bg-[#e5c07b]/15 text-[#e5c07b]">
                  <ListChecks className="size-3.5" />
                </span>
                <span
                  id="inspector-plan-title"
                  className="truncate text-xs font-semibold text-[#e6e6e6]"
                >
                  Plano
                </span>
              </div>
              <div className="flex items-center gap-2">
                <Badge
                  variant="outline"
                  className="h-5 border-[#3e4451] px-1.5 font-mono text-[10px] text-[#7f848e]"
                >
                  {DEFAULT_PLAN.length}
                </Badge>
                <ChevronDown
                  className={`size-3.5 text-[#7f848e] transition-transform duration-200 ${
                    planOpen ? "" : "-rotate-90"
                  }`}
                />
              </div>
            </button>

            {planOpen && (
              <div className="space-y-1.5 border-t border-[#3e4451]/50 bg-[#181a1f]/70 p-2.5 text-xs">
                {DEFAULT_PLAN.map((item) => (
                  <div
                    key={item.id}
                    className="flex items-start gap-2 rounded-md bg-[#21252b]/70 p-2 text-[11px] leading-relaxed text-[#abb2bf]"
                  >
                    {item.completed ? (
                      <CheckCircle2 className="mt-0.5 size-3.5 shrink-0 text-[#98c379]" />
                    ) : (
                      <Circle className="mt-0.5 size-3.5 shrink-0 text-[#7f848e]" />
                    )}
                    <span
                      className={`min-w-0 flex-1 ${
                        item.completed ? "text-[#e6e6e6]" : "text-[#7f848e]"
                      }`}
                    >
                      {item.label}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>

          {/* Card 3: Subagentes */}
          <section
            aria-labelledby="inspector-subagents-title"
            className="overflow-hidden rounded-xl border border-[#3e4451]/80 bg-[#1e2227]/60 shadow-xs transition-colors hover:border-[#3e4451]"
          >
            <button
              type="button"
              aria-expanded={subagentsOpen}
              onClick={() => setSubagentsOpen((prev) => !prev)}
              className="flex w-full cursor-pointer items-center justify-between gap-2 px-3 py-2.5 text-left transition-colors hover:bg-[#2c313a]/50"
            >
              <div className="flex min-w-0 items-center gap-2">
                <span className="flex size-6 items-center justify-center rounded-md bg-[#c678dd]/15 text-[#c678dd]">
                  <Bot className="size-3.5" />
                </span>
                <span
                  id="inspector-subagents-title"
                  className="truncate text-xs font-semibold text-[#e6e6e6]"
                >
                  Subagentes
                </span>
              </div>
              <div className="flex items-center gap-2">
                <Badge
                  variant="outline"
                  className="h-5 border-[#3e4451] px-1.5 font-mono text-[10px] text-[#7f848e]"
                >
                  {DEFAULT_SUBAGENTS.length}
                </Badge>
                <ChevronDown
                  className={`size-3.5 text-[#7f848e] transition-transform duration-200 ${
                    subagentsOpen ? "" : "-rotate-90"
                  }`}
                />
              </div>
            </button>

            {subagentsOpen && (
              <div className="space-y-1.5 border-t border-[#3e4451]/50 bg-[#181a1f]/70 p-2.5 text-xs">
                {DEFAULT_SUBAGENTS.map((agent) => (
                  <div
                    key={agent.name}
                    className="flex items-center justify-between gap-2 rounded-md bg-[#21252b]/70 px-2.5 py-2 text-[11px]"
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <Circle
                        className={`size-2 shrink-0 fill-current ${
                          agent.status === "running"
                            ? "animate-pulse text-[#61afef]"
                            : agent.status === "completed"
                              ? "text-[#98c379]"
                              : "text-[#7f848e]"
                        }`}
                      />
                      <div className="min-w-0">
                        <span className="block truncate font-medium text-[#e6e6e6]">
                          {agent.name}
                        </span>
                        <span className="block truncate text-[10px] text-[#7f848e]">
                          {agent.role}
                        </span>
                      </div>
                    </div>
                    <Badge
                      variant="outline"
                      className={`h-4.5 border-0 px-1.5 text-[9px] ${
                        agent.status === "running"
                          ? "bg-[#61afef]/15 text-[#61afef]"
                          : agent.status === "completed"
                            ? "bg-[#98c379]/15 text-[#98c379]"
                            : "bg-[#2c313a] text-[#7f848e]"
                      }`}
                    >
                      {agent.status === "running"
                        ? "ativo"
                        : agent.status === "completed"
                          ? "concluído"
                          : "ocioso"}
                    </Badge>
                  </div>
                ))}
              </div>
            )}
          </section>
        </div>
      </ScrollArea>

      {/* Footer com uso de contexto */}
      <footer className="shrink-0 border-t border-[#3e4451] p-3.5">
        <div className="mb-2 flex items-center justify-between">
          <h2 className="text-xs font-semibold text-[#e6e6e6]">Contexto</h2>
          <span className="font-mono text-[10px] text-[#7f848e]">38%</span>
        </div>
        <div
          role="progressbar"
          aria-label="Uso de contexto"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={38}
          className="h-1.5 overflow-hidden rounded-full bg-[#3e4451]"
        >
          <div className="h-full w-[38%] rounded-full bg-[#56b6c2]" />
        </div>
        <div className="mt-2 flex items-center justify-between text-[10px] text-[#7f848e]">
          <span>24K tokens</span>
          <span>64K limite</span>
        </div>
      </footer>
    </aside>
  );
}
