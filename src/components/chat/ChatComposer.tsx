import { useRef, useState, type KeyboardEvent } from "react";
import { ArrowUp, Check, ChevronDown, Plus, Square } from "lucide-react";
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
import type { ProviderModel } from "@/core/provider-accounts";
import type { CollaborationMode } from "./types";
import type { TurnOptions } from "@/core/chat";

interface ChatComposerProps {
  onSendMessage: (content: string, options: TurnOptions) => Promise<boolean>;
  onStop?: () => Promise<void>;
  running?: boolean;
  initialOptions?: TurnOptions;
  disabled?: boolean;
  modelGroups: ProviderModelGroup[];
}

const REASONING_LABELS: Record<string, string> = {
  none: "Desativado",
  off: "Desativado",
  minimal: "Mínimo",
  low: "Baixo",
  medium: "Médio",
  high: "Alto",
  xhigh: "Extra alto",
  max: "Máximo",
  ultra: "Ultra",
};

function reasoningLabel(level: string): string {
  return Object.prototype.hasOwnProperty.call(REASONING_LABELS, level)
    ? REASONING_LABELS[level]
    : level;
}

interface ModelOptionDef extends Pick<ProviderModel, "reasoningLevels" | "defaultReasoningLevel"> {
  value: string;
  label: string;
}

export interface ProviderModelGroup {
  provider: string;
  models: ModelOptionDef[];
}

const MODE_OPTIONS = [
  { value: "build", label: "Build (Escrita & Execução)" },
  { value: "plan", label: "Plan (Somente Leitura)" },
] as const;


const MODE_LABELS: Record<CollaborationMode, string> = {
  build: "Build (Escrita & Execução)",
  plan: "Plan (Somente Leitura)",
};

export function ChatComposer({
  onSendMessage,
  modelGroups,
  disabled = false,
  running = false,
  onStop,
  initialOptions,
}: ChatComposerProps) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const sendLock = useRef(false);
  const [selection, setSelection] = useState<{
    model: string;
    reasoning: string | null;
  } | null>(initialOptions ? { model: `${initialOptions.account}/${initialOptions.model}`, reasoning: initialOptions.reasoning } : null);
  const [mode, setMode] = useState<CollaborationMode>(initialOptions?.mode ?? "build");
  const [approvalMode, setApprovalMode] = useState<TurnOptions["approvalMode"]>(initialOptions?.approvalMode ?? "yolo");

  const handleSend = async () => {
    const trimmed = text.trim();
    if (!trimmed || disabled || running || sendLock.current || !currentModelDef) return;
    const separator = currentModelDef.value.indexOf("/");
    if (separator < 1) return;
    sendLock.current = true;
    setSending(true);
    try {
      const accepted = await onSendMessage(trimmed, { account: currentModelDef.value.slice(0, separator), model: currentModelDef.value.slice(separator + 1), reasoning, mode, approvalMode });
      if (accepted) setText("");
    } finally { sendLock.current = false; setSending(false); }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void handleSend();
    }
  };

  const availableModels = modelGroups.flatMap((group) => group.models);
  const currentModelDef =
    availableModels.find((availableModel) => availableModel.value === selection?.model) ??
    availableModels[0];
  const reasoning =
    currentModelDef?.value === selection?.model &&
    selection?.reasoning &&
    currentModelDef?.reasoningLevels.includes(selection.reasoning)
      ? selection.reasoning
      : currentModelDef?.defaultReasoningLevel ?? currentModelDef?.reasoningLevels[0] ?? null;
  const displayModelLabel = currentModelDef
    ? reasoning
      ? `${currentModelDef.label} · ${reasoningLabel(reasoning)}`
      : currentModelDef.label
    : "Nenhum modelo conectado";

  return (
    <div className="w-full">
      <div role="group" aria-label="Mensagem e opções de envio" data-working={running || undefined} className="chat-composer relative isolate w-full rounded-[22px] border border-[#3e4451] bg-[#21252b] shadow-2xl shadow-black/30 transition-all focus-within:border-[#61afef]/60 focus-within:ring-1 focus-within:ring-[#61afef]/30">
        {/* Textarea no topo com padding interno espaçoso longe das extremidades */}
        <div className="w-full">
          <Textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={disabled || sending || running || !currentModelDef}
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
            disabled
            title="Anexos ainda não disponíveis"
            aria-label="Adicionar anexo"
            className="size-7.5 cursor-pointer rounded-full bg-[#2c313a] text-[#abb2bf] transition-colors hover:bg-[#3e4451] hover:text-[#e6e6e6]"
          >
            <Plus className="size-3.5 stroke-[2.2]" />
          </Button>

          {/* Canto inferior direito: seletor de modo/agente, seletor de modelo e botão redondo de envio */}
          <div className="flex items-center gap-1 sm:gap-2">
            <DropdownMenu>
              <DropdownMenuTrigger disabled={running || sending} aria-label="Selecionar autorização de ferramentas" className="flex h-7.5 cursor-pointer items-center gap-1 rounded-md px-2 text-xs font-medium text-muted-foreground hover:bg-accent">
                {approvalMode === "manual" ? "Manual" : "YOLO"}<ChevronDown className="size-3" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" side="top" className="max-w-80">
                <DropdownMenuGroup>
                  <DropdownMenuLabel>Autorização no modo Build</DropdownMenuLabel>
                  <DropdownMenuItem className="cursor-pointer" onClick={() => setApprovalMode("manual")}>
                    Manual — confirmar edições e comandos {approvalMode === "manual" && <Check className="size-3" />}
                  </DropdownMenuItem>
                  <DropdownMenuItem className="cursor-pointer" onClick={() => setApprovalMode("yolo")}>
                    YOLO — executar automaticamente {approvalMode === "yolo" && <Check className="size-3" />}
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuLabel className="whitespace-normal text-xs font-normal text-muted-foreground">Comandos iniciam na pasta do projeto e usam as permissões do seu usuário no computador.</DropdownMenuLabel>
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
            {/* Seletor do Agente/Modo (mesmo padrão DropdownMenu do seletor de modelo) */}
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label="Selecionar modo de execução"
                disabled={running || sending}
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
                disabled={running || sending}
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
                {modelGroups.length === 0 ? (
                  <DropdownMenuGroup>
                    <DropdownMenuLabel className="px-2.5 py-2 text-xs font-normal text-[#7f848e]">
                      Conecte um provedor em Configurações.
                    </DropdownMenuLabel>
                  </DropdownMenuGroup>
                ) : (
                  modelGroups.map((group, groupIndex) => (
                    <div key={group.provider}>
                      {groupIndex > 0 && (
                        <DropdownMenuSeparator className="my-1.5 bg-[#3e4451]" />
                      )}
                      <DropdownMenuGroup>
                        <DropdownMenuLabel className="px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-[#56b6c2]">
                          {group.provider}
                        </DropdownMenuLabel>
                        {group.models.map((option) => {
                          const isSelected = option.value === currentModelDef?.value;

                          if (option.reasoningLevels.length > 0) {
                            return (
                              <DropdownMenuSub key={option.value}>
                                <DropdownMenuSubTrigger
                                  className={`cursor-pointer py-1.5 pl-3 pr-2 text-xs hover:bg-[#2c313a] ${
                                    isSelected
                                      ? "font-medium text-[#61afef]"
                                      : "text-[#abb2bf]"
                                  }`}
                                >
                                  <span className="flex-1 truncate">{option.label}</span>
                                  {isSelected && reasoning && (
                                    <span className="mr-1 text-[10px] text-[#7f848e]">
                                      {reasoningLabel(reasoning)}
                                    </span>
                                  )}
                                </DropdownMenuSubTrigger>
                                <DropdownMenuSubContent className="min-w-[130px] border-[#3e4451] bg-[#21252b] p-1 text-[#e6e6e6]">
                                  <DropdownMenuGroup>
                                    <DropdownMenuLabel className="px-2 py-1 text-[10px] font-medium text-[#7f848e]">
                                      Raciocínio
                                    </DropdownMenuLabel>
                                    {option.reasoningLevels.map((level) => (
                                      <DropdownMenuItem
                                        key={level}
                                        onClick={() => {
                                          setSelection({ model: option.value, reasoning: level });
                                        }}
                                        className={`flex cursor-pointer items-center justify-between px-2.5 py-1.5 text-xs hover:bg-[#2c313a] ${
                                          isSelected && reasoning === level
                                            ? "bg-[#2c313a]/50 font-medium text-[#61afef]"
                                            : "text-[#abb2bf]"
                                        }`}
                                      >
                                        <span>{reasoningLabel(level)}</span>
                                        {isSelected && reasoning === level && (
                                          <Check className="size-3 text-[#61afef]" />
                                        )}
                                      </DropdownMenuItem>
                                    ))}
                                  </DropdownMenuGroup>
                                </DropdownMenuSubContent>
                              </DropdownMenuSub>
                            );
                          }

                          return (
                            <DropdownMenuItem
                              key={option.value}
                              onClick={() => {
                                setSelection({ model: option.value, reasoning: null });
                              }}
                              className={`flex cursor-pointer items-center justify-between py-1.5 pl-3 pr-2 text-xs hover:bg-[#2c313a] ${
                                isSelected
                                  ? "font-medium text-[#61afef]"
                                  : "text-[#abb2bf]"
                              }`}
                            >
                              <span>{option.label}</span>
                              {isSelected && <Check className="size-3 text-[#61afef]" />}
                            </DropdownMenuItem>
                          );
                        })}
                      </DropdownMenuGroup>
                    </div>
                  ))
                )}
              </DropdownMenuContent>
            </DropdownMenu>

            {/* Botão redondo com seta pra cima no canto inferior direito */}
            {running ? <Button type="button" size="icon" variant="destructive" className="size-7.5 cursor-pointer rounded-full" aria-label="Interromper execução" onClick={() => { void onStop?.(); }}><Square className="size-3.5" /></Button> : <Button
              type="button"
              size="icon"
              onClick={() => { void handleSend(); }}
              disabled={!text.trim() || disabled || sending || !currentModelDef}
              aria-label="Enviar mensagem"
              className={`size-7.5 cursor-pointer rounded-full transition-all ${
                text.trim()
                  ? "bg-[#61afef] text-[#1e2227] shadow-sm shadow-[#61afef]/30 hover:bg-[#61afef]/90 active:scale-95"
                  : "cursor-not-allowed bg-[#2c313a] text-[#7f848e]"
              }`}
            >
              <ArrowUp className="size-3.5 stroke-[2.5]" />
            </Button>}
          </div>
        </div>
      </div>
    </div>
  );
}
