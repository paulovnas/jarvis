import { useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  ChevronRight,
  Code2,
  FileCode2,
  Loader2,
  Terminal,
  Wrench,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import type { ToolCallItem } from "./types";

interface ToolCallCardProps {
  tool: ToolCallItem;
}
function renderToolIcon(name: string) {
  const lower = name.toLowerCase();
  if (lower.includes("bash") || lower.includes("exec") || lower.includes("cargo")) {
    return <Terminal className="size-3.5 shrink-0 text-[#56b6c2]" aria-hidden="true" />;
  }
  if (lower.includes("read") || lower.includes("edit") || lower.includes("file")) {
    return <FileCode2 className="size-3.5 shrink-0 text-[#56b6c2]" aria-hidden="true" />;
  }
  if (lower.includes("code") || lower.includes("diff")) {
    return <Code2 className="size-3.5 shrink-0 text-[#56b6c2]" aria-hidden="true" />;
  }
  return <Wrench className="size-3.5 shrink-0 text-[#56b6c2]" aria-hidden="true" />;
}
function formatToolTitle(name: string): string {
  const map: Record<string, string> = {
    read_file: "Leitura de arquivo",
    edit_file: "Edição de código",
    bash: "Execução no terminal",
    bun_test: "Execução de testes",
    cargo_check: "Verificação de integridade Rust",
    cargo_test: "Testes de persistência Cargo",
    curl_check: "Verificação de serviço HTTP",
  };
  return map[name] || name;
}

export function ToolCallCard({ tool }: ToolCallCardProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div
      data-testid={`tool-call-${tool.id}`}
      className="overflow-hidden rounded-lg border border-[#3e4451]/60 bg-[#1e2227]/70 transition-colors hover:border-[#3e4451]"
    >
      <button
        type="button"
        aria-expanded={expanded}
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full cursor-pointer items-center justify-between gap-3 px-3.5 py-2.5 text-left transition-colors hover:bg-[#2c313a]/50"
      >
        <div className="flex min-w-0 items-center gap-2.5">
          {renderToolIcon(tool.name)}
          <span className="truncate text-[13px] font-medium text-[#e6e6e6]">
            {formatToolTitle(tool.name)}
          </span>
          <code className="hidden rounded bg-[#282c34] px-1.5 py-0.5 font-mono text-[10px] text-[#7f848e] sm:inline">
            {tool.name}
          </code>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {tool.diffStats && (
            <span className="flex items-center gap-1 font-mono text-[11px]">
              {tool.diffStats.added > 0 && (
                <span className="text-[#98c379]">+{tool.diffStats.added}</span>
              )}
              {tool.diffStats.removed > 0 && (
                <span className="text-[#e06c75]">-{tool.diffStats.removed}</span>
              )}
            </span>
          )}

          {tool.durationMs !== undefined && (
            <span className="font-mono text-[10px] text-[#7f848e]">
              {tool.durationMs < 1000
                ? `${tool.durationMs}ms`
                : `${(tool.durationMs / 1000).toFixed(1)}s`}
            </span>
          )}

          {tool.status === "completed" && (
            <Badge
              variant="outline"
              className="h-5 gap-1 border-[#98c379]/40 bg-[#98c379]/10 px-1.5 text-[10px] text-[#98c379]"
            >
              <CheckCircle2 className="size-2.5" />
              Sucesso
            </Badge>
          )}

          {tool.status === "running" && (
            <Badge
              variant="outline"
              className="h-5 gap-1 border-[#61afef]/40 bg-[#61afef]/10 px-1.5 text-[10px] text-[#61afef]"
            >
              <Loader2 className="size-2.5 animate-spin" />
              Executando
            </Badge>
          )}

          {tool.status === "error" && (
            <Badge
              variant="outline"
              className="h-5 gap-1 border-[#e06c75]/40 bg-[#e06c75]/10 px-1.5 text-[10px] text-[#e06c75]"
            >
              <AlertCircle className="size-2.5" />
              Falha
            </Badge>
          )}

          <ChevronRight
            aria-hidden="true"
            className={`size-3.5 text-[#7f848e] transition-transform duration-200 ${
              expanded ? "rotate-90" : ""
            }`}
          />
        </div>
      </button>
      {expanded && (
        <div className="space-y-3 border-t border-[#3e4451]/50 bg-[#181a1f] p-3.5 text-xs sm:p-4">
          {tool.args && (
            <div>
              <span className="mb-1.5 block font-mono text-[10px] uppercase tracking-wider text-[#7f848e]">
                Parâmetros
              </span>
              <pre className="max-h-40 overflow-x-auto rounded-md border border-[#3e4451]/40 bg-[#21252b] p-3 font-mono text-[11.5px] leading-relaxed text-[#abb2bf]">
                {JSON.stringify(tool.args, null, 2)}
              </pre>
            </div>
          )}

          {tool.output && (
            <div>
              <span className="mb-1.5 block font-mono text-[10px] uppercase tracking-wider text-[#7f848e]">
                Saída do terminal
              </span>
              <pre className="max-h-44 overflow-x-auto rounded-md border border-[#3e4451]/40 bg-[#14161a] p-3 font-mono text-[11.5px] leading-relaxed text-[#98c379]/90">
                {tool.output}
              </pre>
            </div>
          )}

          {tool.error && (
            <div className="rounded-md border border-[#e06c75]/40 bg-[#e06c75]/10 p-3 text-[#e06c75]">
              <span className="mb-1 block font-semibold text-[11px]">
                Erro de execução:
              </span>
              <p className="font-mono text-[11.5px] leading-relaxed">{tool.error}</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
