import { useState } from "react";
import { BrainCircuit, ChevronRight, Sparkles, Wrench } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { ToolCallCard } from "./ToolCallCard";
import type { AssistantWorkData } from "./types";

interface AssistantWorkCollapseProps {
  work: AssistantWorkData;
  isStreaming?: boolean;
}

export function AssistantWorkCollapse({
  work,
  isStreaming = false,
}: AssistantWorkCollapseProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  const completedToolsCount = work.tools.filter(
    (tool) => tool.status === "completed"
  ).length;

  return (
    <section
      aria-label="Processamento do Jarvis"
      className="my-3 w-full min-w-0 rounded-xl border border-[#3e4451]/70 bg-[#21252b]/90 shadow-sm transition-all"
    >
      <button
        type="button"
        aria-expanded={isExpanded}
        onClick={() => setIsExpanded((prev) => !prev)}
        className="flex w-full cursor-pointer items-center justify-between gap-3 px-4 py-3 text-left text-xs transition-colors hover:bg-[#2c313a]/50 sm:text-[13px]"
      >
        <div className="flex min-w-0 items-center gap-2.5">
          <span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-[#61afef]/15 text-[#61afef]">
            {isStreaming ? (
              <Sparkles className="size-4 animate-pulse" />
            ) : (
              <BrainCircuit className="size-4 text-[#c678dd]" />
            )}
          </span>
          <span className="font-medium text-[#e6e6e6]">
            {isStreaming
              ? "Pensando e executando tarefas…"
              : `Trabalhou por ${work.durationSeconds}s`}
          </span>
          <Badge
            variant="outline"
            className="hidden border-[#3e4451] text-[10px] text-[#7f848e] sm:inline-flex"
          >
            {work.tools.length} ações
          </Badge>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {completedToolsCount > 0 && (
            <span className="text-[11px] text-[#98c379]">
              {completedToolsCount} concluída{completedToolsCount > 1 ? "s" : ""}
            </span>
          )}
          <span className="text-[11px] text-[#7f848e]">
            {isExpanded ? "Ocultar detalhes" : "Ver detalhes"}
          </span>
          <ChevronRight
            aria-hidden="true"
            className={`size-3.5 text-[#7f848e] transition-transform duration-200 ${
              isExpanded ? "rotate-90" : ""
            }`}
          />
        </div>
      </button>

      {isExpanded && (
        <div className="space-y-4 border-t border-[#3e4451]/60 bg-[#1e2227]/40 p-4 sm:p-5">
          {work.thinking && (
            <div className="rounded-xl border border-[#c678dd]/30 bg-[#c678dd]/5 p-4 text-[13.5px]">
              <div className="mb-2 flex items-center gap-2 font-medium text-[#c678dd]">
                <BrainCircuit className="size-4" />
                <span>Raciocínio interno do agente</span>
              </div>
              <p className="whitespace-pre-wrap break-words leading-relaxed text-[#abb2bf]">
                {work.thinking}
              </p>
            </div>
          )}

          {work.tools.length > 0 && (
            <div className="space-y-2">
              <div className="flex items-center justify-between text-[11px] text-[#7f848e]">
                <span className="flex items-center gap-1 font-semibold uppercase tracking-wider">
                  <Wrench className="size-3 text-[#56b6c2]" />
                  Chamadas de ferramentas
                </span>
                <span>{work.tools.length} no total</span>
              </div>
              <div className="space-y-1.5">
                {work.tools.map((tool) => (
                  <ToolCallCard key={tool.id} tool={tool} />
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
