import { useState, type KeyboardEvent } from "react";
import { ArrowUp, Check, ChevronDown, Plus } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Textarea } from "@/components/ui/textarea";
import type { AiModel, CollaborationMode } from "./types";

interface ChatComposerProps {
  onSendMessage: (content: string) => void;
  disabled?: boolean;
}

export type ReasoningEffort = "off" | "low" | "medium" | "high";

const REASONING_LEVELS: { value: ReasoningEffort; label: string }[] = [
  { value: "off", label: "Desativado" },
  { value: "low", label: "Baixo" },
  { value: "medium", label: "Médio" },
  { value: "high", label: "Alto" },
];

const REASONING_LABELS: Record<ReasoningEffort, string> = {
  off: "Desativado",
  low: "Baixo",
  medium: "Médio",
  high: "Alto",
};

interface ModelOptionDef {
  value: AiModel;
  label: string;
  hasReasoning: boolean;
}

interface ProviderGroupDef {
  provider: string;
  models: ModelOptionDef[];
}

const MODEL_GROUPS: ProviderGroupDef[] = [
  {
    provider: "Antigravity",
    models: [
      { value: "gemini-2.5-pro", label: "Gemini 2.5 Pro", hasReasoning: true },
      { value: "gemini-2.5-flash", label: "Gemini 2.5 Flash", hasReasoning: true },
      {
        value: "gemini-2.0-flash-thinking",
        label: "Gemini 2.0 Flash Thinking",
        hasReasoning: true,
      },
    ],
  },
  {
    provider: "OpenAI",
    models: [
      { value: "gpt-4o", label: "GPT-4o", hasReasoning: false },
      { value: "gpt-4o-mini", label: "GPT-4o Mini", hasReasoning: false },
      { value: "o3-mini", label: "o3-mini", hasReasoning: true },
    ],
  },
];

const MODE_OPTIONS = [
  { value: "build", label: "Build (Escrita & Execução)" },
  { value: "plan", label: "Plan (Somente Leitura)" },
] as const;

const MODEL_LABELS: Record<AiModel, string> = {
  "gemini-2.5-pro": "Gemini 2.5 Pro",
  "gemini-2.5-flash": "Gemini 2.5 Flash",
  "gemini-2.0-flash-thinking": "Gemini 2.0 Flash Thinking",
  "gpt-4o": "GPT-4o",
  "gpt-4o-mini": "GPT-4o Mini",
  "o3-mini": "o3-mini",
};

const MODE_LABELS: Record<CollaborationMode, string> = {
  build: "Build (Escrita & Execução)",
  plan: "Plan (Somente Leitura)",
};

export function ChatComposer({
  onSendMessage,
  disabled = false,
}: ChatComposerProps) {
  const [text, setText] = useState("");
  const [model, setModel] = useState<AiModel>("gemini-2.5-pro");
  const [reasoning, setReasoning] = useState<ReasoningEffort>("high");
  const [mode, setMode] = useState<CollaborationMode>("build");

  const handleSend = () => {
    const trimmed = text.trim();
    if (!trimmed || disabled) return;
    onSendMessage(trimmed);
    setText("");
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleAttachment = () => {
    toast.info("Anexo de arquivo (mock)", {
      description: "Integração para anexar arquivos e contexto local.",
    });
  };

  // Se o modelo suportar raciocínio e não estiver desativado, exibe "Modelo · Nível"
  const currentModelDef = MODEL_GROUPS.flatMap((g) => g.models).find(
    (m) => m.value === model
  );
  const displayModelLabel =
    currentModelDef?.hasReasoning && reasoning !== "off"
      ? `${MODEL_LABELS[model]} · ${REASONING_LABELS[reasoning]}`
      : MODEL_LABELS[model];

  return (
    <div className="w-full">
      <div className="w-full rounded-[22px] border border-[#3e4451] bg-[#21252b] shadow-2xl shadow-black/30 transition-all focus-within:border-[#61afef]/60 focus-within:ring-1 focus-within:ring-[#61afef]/30">
        {/* Textarea no topo com padding interno espaçoso longe das extremidades */}
        <div className="w-full">
          <Textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={disabled}
            placeholder="Pergunte ou dê uma instrução ao Jarvis (ex: 'refatore os testes do backend')..."
            className="min-h-[84px] w-full resize-none border-0 bg-transparent px-5 pt-4 pb-2 text-[14.5px] leading-relaxed text-[#e6e6e6] placeholder:text-[#7f848e] placeholder:leading-relaxed focus-visible:ring-0 focus-visible:outline-none"
          />
        </div>

        {/* Linha de controles inferior no padrão Metis */}
        <div className="flex items-center justify-between px-3.5 pb-2.5 pt-1">
          {/* Canto inferior esquerdo: botão de anexo com ícone plus */}
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={handleAttachment}
            aria-label="Adicionar anexo"
            className="size-7.5 cursor-pointer rounded-full bg-[#2c313a] text-[#abb2bf] transition-colors hover:bg-[#3e4451] hover:text-[#e6e6e6]"
          >
            <Plus className="size-3.5 stroke-[2.2]" />
          </Button>

          {/* Canto inferior direito: seletor de modo/agente, seletor de modelo e botão redondo de envio */}
          <div className="flex items-center gap-1 sm:gap-2">
            {/* Seletor do Agente/Modo (mesmo padrão DropdownMenu do seletor de modelo) */}
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label="Selecionar modo de execução"
                className="flex h-7.5 cursor-pointer items-center gap-1 rounded-md border-0 bg-transparent px-2 text-xs font-medium text-[#abb2bf] shadow-none transition-colors hover:bg-[#2c313a] hover:text-[#e6e6e6] focus-visible:ring-1 focus-visible:ring-[#3e4451]"
              >
                <span className="truncate">{MODE_LABELS[mode]}</span>
                <ChevronDown className="size-3 shrink-0 text-[#7f848e]" />
              </DropdownMenuTrigger>

              <DropdownMenuContent
                align="end"
                side="top"
                sideOffset={8}
                className="min-w-[190px] border-[#3e4451] bg-[#21252b] p-1.5 text-[#e6e6e6]"
              >
                <DropdownMenuGroup>
                  <DropdownMenuLabel className="px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-[#e5c07b]">
                    Modo do Agente
                  </DropdownMenuLabel>
                  {MODE_OPTIONS.map((opt) => {
                    const isSelected = opt.value === mode;
                    return (
                      <DropdownMenuItem
                        key={opt.value}
                        onClick={() => setMode(opt.value)}
                        className={`flex cursor-pointer items-center justify-between py-1.5 pl-3 pr-2 text-xs hover:bg-[#2c313a] ${
                          isSelected
                            ? "font-medium text-[#61afef]"
                            : "text-[#abb2bf]"
                        }`}
                      >
                        <span>{opt.label}</span>
                        {isSelected && (
                          <Check className="size-3 text-[#61afef]" />
                        )}
                      </DropdownMenuItem>
                    );
                  })}
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>

            {/* Seletor de Modelo com submenu de raciocínio no hover (padrão Metis) */}
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label="Selecionar modelo de IA"
                className="flex h-7.5 cursor-pointer items-center gap-1 rounded-md border-0 bg-transparent px-2 text-xs font-medium text-[#abb2bf] shadow-none transition-colors hover:bg-[#2c313a] hover:text-[#e6e6e6] focus-visible:ring-1 focus-visible:ring-[#3e4451]"
              >
                <span className="truncate">{displayModelLabel}</span>
                <ChevronDown className="size-3 shrink-0 text-[#7f848e]" />
              </DropdownMenuTrigger>

              <DropdownMenuContent
                align="end"
                side="top"
                sideOffset={8}
                className="min-w-[220px] border-[#3e4451] bg-[#21252b] p-1.5 text-[#e6e6e6]"
              >
                {MODEL_GROUPS.map((group, gIdx) => (
                  <div key={group.provider}>
                    {gIdx > 0 && (
                      <DropdownMenuSeparator className="my-1.5 bg-[#3e4451]" />
                    )}
                    <DropdownMenuGroup>
                      <DropdownMenuLabel className="px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-[#56b6c2]">
                        {group.provider}
                      </DropdownMenuLabel>
                      {group.models.map((opt) => {
                        const isSelected = opt.value === model;

                        if (opt.hasReasoning) {
                          return (
                            <DropdownMenuSub key={opt.value}>
                              <DropdownMenuSubTrigger
                                className={`cursor-pointer py-1.5 pl-3 pr-2 text-xs hover:bg-[#2c313a] ${
                                  isSelected
                                    ? "font-medium text-[#61afef]"
                                    : "text-[#abb2bf]"
                                }`}
                              >
                                <span className="flex-1 truncate">{opt.label}</span>
                                {isSelected && reasoning !== "off" && (
                                  <span className="mr-1 text-[10px] text-[#7f848e]">
                                    {REASONING_LABELS[reasoning]}
                                  </span>
                                )}
                              </DropdownMenuSubTrigger>
                              <DropdownMenuSubContent className="min-w-[130px] border-[#3e4451] bg-[#21252b] p-1 text-[#e6e6e6]">
                                <DropdownMenuLabel className="px-2 py-1 text-[10px] font-medium text-[#7f848e]">
                                  Raciocínio
                                </DropdownMenuLabel>
                                {REASONING_LEVELS.map((level) => (
                                  <DropdownMenuItem
                                    key={level.value}
                                    onClick={() => {
                                      setModel(opt.value);
                                      setReasoning(level.value);
                                    }}
                                    className={`flex cursor-pointer items-center justify-between px-2.5 py-1.5 text-xs hover:bg-[#2c313a] ${
                                      isSelected && reasoning === level.value
                                        ? "bg-[#2c313a]/50 font-medium text-[#61afef]"
                                        : "text-[#abb2bf]"
                                    }`}
                                  >
                                    <span>{level.label}</span>
                                    {isSelected && reasoning === level.value && (
                                      <Check className="size-3 text-[#61afef]" />
                                    )}
                                  </DropdownMenuItem>
                                ))}
                              </DropdownMenuSubContent>
                            </DropdownMenuSub>
                          );
                        }

                        return (
                          <DropdownMenuItem
                            key={opt.value}
                            onClick={() => {
                              setModel(opt.value);
                              setReasoning("off");
                            }}
                            className={`flex cursor-pointer items-center justify-between py-1.5 pl-3 pr-2 text-xs hover:bg-[#2c313a] ${
                              isSelected
                                ? "font-medium text-[#61afef]"
                                : "text-[#abb2bf]"
                            }`}
                          >
                            <span>{opt.label}</span>
                            {isSelected && (
                              <Check className="size-3 text-[#61afef]" />
                            )}
                          </DropdownMenuItem>
                        );
                      })}
                    </DropdownMenuGroup>
                  </div>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>

            {/* Botão redondo com seta pra cima no canto inferior direito */}
            <Button
              type="button"
              size="icon"
              onClick={handleSend}
              disabled={!text.trim() || disabled}
              aria-label="Enviar mensagem"
              className={`size-7.5 cursor-pointer rounded-full transition-all ${
                text.trim()
                  ? "bg-[#61afef] text-[#1e2227] shadow-sm shadow-[#61afef]/30 hover:bg-[#61afef]/90 active:scale-95"
                  : "cursor-not-allowed bg-[#2c313a] text-[#7f848e]"
              }`}
            >
              <ArrowUp className="size-3.5 stroke-[2.5]" />
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
