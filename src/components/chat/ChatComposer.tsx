import { lazy, Suspense, useRef, useState } from "react";
import { ArrowUp, Check, ChevronDown, Plus, Square, ListOrdered, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ComposerSkeleton } from "@/components/layout/LoadingSkeletons";
import { MessageContent } from "./MessageContent";
import type { ProviderModel } from "@/core/provider-accounts";
import type { CollaborationMode } from "./types";
import { mergeDrafts, type ChatDraft, type MessagePart, type QueuedMessage, type TurnOptions } from "@/core/chat";

const SkillInput = lazy(() => import("./SkillInput").then(module => ({ default: module.SkillInput })));

interface ChatComposerProps {
  onSendMessage: (content: string, options: TurnOptions, parts?: MessagePart[]) => Promise<boolean>;
  onStop?: () => Promise<void>;
  running?: boolean;
  compacting?: boolean;
  initialOptions?: TurnOptions;
  disabled?: boolean;
  modelGroups: ProviderModelGroup[];
  draftKey?: string;
  drafts?: Map<string, ChatDraft>;
  queuedMessages?: QueuedMessage[];
  onRemoveQueued?: (id: string) => Promise<ChatDraft | null>;
  onResumeQueue?: () => Promise<void>;
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
  build: "Build",
  plan: "Plan",
};

export function ChatComposer({
  onSendMessage,
  modelGroups,
  disabled = false,
  running = false,
  compacting = false,
  onStop,
  initialOptions,
  draftKey,
  drafts,
  queuedMessages = [],
  onRemoveQueued,
  onResumeQueue,
}: ChatComposerProps) {
  const [draft, updateDraft] = useState<ChatDraft>(() => draftKey ? drafts?.get(draftKey) ?? { content: "" } : { content: "" });
  const text = draft.content;
  const draftRef = useRef(draft);
  const input = useRef<{ focus: () => void }>(null);
  const [removing, setRemoving] = useState<string[]>([]);
  const removeLocks = useRef(new Set<string>());
  const [resuming, setResuming] = useState(false);
  const setDraft = (value: ChatDraft) => {
    draftRef.current = value;
    if (draftKey) { if (value.content) drafts?.set(draftKey, value); else drafts?.delete(draftKey); }
    updateDraft(value);
  };
  const removeQueued = async (id: string) => {
    if (!onRemoveQueued || removeLocks.current.has(id)) return;
    removeLocks.current.add(id); setRemoving([...removeLocks.current]);
    try {
      const restored = await onRemoveQueued(id);
      if (restored !== null) {
        const current = draftKey && drafts ? drafts.get(draftKey) ?? { content: "" } : draftRef.current;
        setDraft(mergeDrafts(current, restored));
        input.current?.focus();
      }
    } finally { removeLocks.current.delete(id); setRemoving([...removeLocks.current]); }
  };
  const [sending, setSending] = useState(false);
  const sendLock = useRef(false);
  const [selection, setSelection] = useState<{
    model: string;
    reasoning: string | null;
  } | null>(initialOptions ? { model: `${initialOptions.account}/${initialOptions.model}`, reasoning: initialOptions.reasoning } : null);
  const [mode, setMode] = useState<CollaborationMode>(initialOptions?.mode ?? "build");
  const [approvalMode, setApprovalMode] = useState<TurnOptions["approvalMode"]>(initialOptions?.approvalMode ?? "yolo");

  const handleSend = async () => {
    const submitted = draftRef.current;
    const trimmed = submitted.content.trim();
    if (!trimmed || disabled || compacting || sendLock.current || !currentModelDef) return;
    const separator = currentModelDef.value.indexOf("/");
    if (separator < 1) return;
    sendLock.current = true;
    setSending(true);
    try {
      const options = running && initialOptions ? initialOptions : { account: currentModelDef.value.slice(0, separator), model: currentModelDef.value.slice(separator + 1), reasoning, mode, approvalMode };
      const accepted = submitted.parts?.length ? await onSendMessage(trimmed, options, submitted.parts) : await onSendMessage(trimmed, options);
      const current = draftKey && drafts ? drafts.get(draftKey) ?? { content: "" } : draftRef.current;
      if (accepted && JSON.stringify(current) === JSON.stringify(submitted)) setDraft({ content: "" });
    } finally { sendLock.current = false; setSending(false); }
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
      {queuedMessages.length > 0 && <section aria-label="Mensagens agendadas" className="mx-3 rounded-t-xl border border-b-0 border-border bg-card px-3 py-2">
        <div className="mb-1 flex items-center gap-2 text-[11px] text-muted-foreground"><ListOrdered className="size-3.5" /><span>{running ? "Após a resposta atual" : "Fila pausada"} · {queuedMessages.length}</span>
          {!running && <Button variant="ghost" size="sm" className="ml-auto h-6 cursor-pointer text-[11px]" disabled={resuming || compacting} onClick={() => { setResuming(true); void onResumeQueue?.().finally(() => setResuming(false)); }}>Continuar fila</Button>}
        </div>
        <div className="max-h-36 overflow-y-auto">{queuedMessages.map((message, index) => <div key={message.id} className="flex items-center gap-2 border-t border-border/50 py-1.5 text-xs">
          <span className="text-muted-foreground tabular-nums">{index + 1}</span><p className="min-w-0 flex-1 truncate" title={message.content}><MessageContent content={message.content} parts={message.parts} /></p>
          <Button variant="ghost" size="icon" className="size-6 shrink-0 cursor-pointer text-muted-foreground" aria-label={`Retirar mensagem ${index + 1} e editar`} title="Cancelar envio e devolver ao campo de texto" disabled={compacting || removing.includes(message.id)} onClick={() => { void removeQueued(message.id); }}><X className="size-3.5" /></Button>
        </div>)}</div>
      </section>}
      <Suspense fallback={<ComposerSkeleton />}><SkillInput ref={input} draft={draft} onChange={setDraft} onSend={() => { void handleSend(); }} disabled={disabled || compacting} compacting={compacting} working={running || compacting}>

        {/* Linha de controles inferior no padrão Metis */}
        <div className="composer-controls flex w-full items-center justify-between gap-1 px-3 pb-3 pt-1">
          {/* Canto inferior esquerdo: botão de anexo com ícone plus */}
          <Button
            type="button"
            variant="ghost"
            size="icon"
            disabled
            title="Anexos ainda não disponíveis"
            aria-label="Adicionar anexo"
            className="size-7.5 cursor-pointer rounded-full bg-secondary text-foreground transition-colors hover:bg-accent hover:text-foreground"
          >
            <Plus className="size-3.5 stroke-[2.2]" />
          </Button>

          {/* Canto inferior direito: seletor de modo/agente, seletor de modelo e botão redondo de envio */}
          <div className="composer-options flex flex-1 items-center gap-0.5">
            <DropdownMenu>
              <DropdownMenuTrigger disabled={running || sending || compacting} aria-label="Selecionar autorização de ferramentas" className="flex h-7.5 cursor-pointer items-center gap-1 rounded-md px-2 text-xs font-medium text-muted-foreground hover:bg-accent">
                {approvalMode === "manual" ? "Manual" : "YOLO"}<ChevronDown className="size-3" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" side="top" className="max-w-80">
                <DropdownMenuGroup>
                  <DropdownMenuItem className="cursor-pointer" onClick={() => setApprovalMode("manual")}>
                    Manual {approvalMode === "manual" && <Check className="size-3" />}
                  </DropdownMenuItem>
                  <DropdownMenuItem className="cursor-pointer" onClick={() => setApprovalMode("yolo")}>
                    YOLO {approvalMode === "yolo" && <Check className="size-3" />}
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
            {/* Seletor do Agente/Modo (mesmo padrão DropdownMenu do seletor de modelo) */}
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label="Selecionar modo de execução"
                disabled={running || sending || compacting}
                className="flex h-7.5 cursor-pointer items-center gap-1 rounded-md border-0 bg-transparent px-2 text-xs font-medium text-foreground shadow-none transition-colors hover:bg-secondary hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring"
              >
                <span className="truncate">{MODE_LABELS[mode]}</span>
                <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
              </DropdownMenuTrigger>

              <DropdownMenuContent
                align="end"
                side="top"
                sideOffset={8}
                className="min-w-[190px] border-border bg-card p-1.5 text-foreground"
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
                        className={`flex cursor-pointer items-center justify-between py-1.5 pl-3 pr-2 text-xs hover:bg-secondary ${
                          isSelected
                            ? "font-medium text-[#61afef]"
                            : "text-foreground"
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
                disabled={running || sending || compacting}
                className="composer-model flex h-7.5 cursor-pointer items-center gap-1 rounded-md border-0 bg-transparent px-2 font-mono text-[10px] font-medium text-foreground shadow-none transition-colors hover:bg-secondary hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring"
              >
                <span className="truncate">{displayModelLabel}</span>
                <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
              </DropdownMenuTrigger>

              <DropdownMenuContent
                align="end"
                side="top"
                sideOffset={8}
                className="min-w-[220px] border-border bg-card p-1.5 text-foreground"
              >
                {modelGroups.length === 0 ? (
                  <DropdownMenuGroup>
                    <DropdownMenuLabel className="px-2.5 py-2 text-xs font-normal text-muted-foreground">
                      Conecte um provedor em Configurações.
                    </DropdownMenuLabel>
                  </DropdownMenuGroup>
                ) : (
                  modelGroups.map((group) => (
                    <DropdownMenuSub key={group.provider}>
                        <DropdownMenuSubTrigger className="cursor-pointer gap-3 py-2 font-mono text-xs text-[#56b6c2]">
                          {group.provider}
                        </DropdownMenuSubTrigger>
                      <DropdownMenuSubContent className="max-h-[min(480px,70vh)] min-w-[220px] overflow-y-auto border-border bg-card p-1.5 text-foreground">
                        {group.models.map((option) => {
                          const isSelected = option.value === currentModelDef?.value;

                          if (option.reasoningLevels.length > 0) {
                            return (
                              <DropdownMenuSub key={option.value}>
                                <DropdownMenuSubTrigger
                                  className={`cursor-pointer py-1.5 pl-3 pr-2 text-xs hover:bg-secondary ${
                                    isSelected
                                      ? "font-medium text-[#61afef]"
                                      : "text-foreground"
                                  }`}
                                >
                                  <span className="flex-1 truncate">{option.label}</span>
                                  {isSelected && reasoning && (
                                    <span className="mr-1 text-[10px] text-muted-foreground">
                                      {reasoningLabel(reasoning)}
                                    </span>
                                  )}
                                </DropdownMenuSubTrigger>
                                <DropdownMenuSubContent className="min-w-[130px] border-border bg-card p-1 text-foreground">
                                  <DropdownMenuGroup>
                                    <DropdownMenuLabel className="px-2 py-1 text-[10px] font-medium text-muted-foreground">
                                      Raciocínio
                                    </DropdownMenuLabel>
                                    {option.reasoningLevels.map((level) => (
                                      <DropdownMenuItem
                                        key={level}
                                        onClick={() => {
                                          setSelection({ model: option.value, reasoning: level });
                                        }}
                                        className={`flex cursor-pointer items-center justify-between px-2.5 py-1.5 text-xs hover:bg-secondary ${
                                          isSelected && reasoning === level
                                            ? "bg-secondary/50 font-medium text-[#61afef]"
                                            : "text-foreground"
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
                              className={`flex cursor-pointer items-center justify-between py-1.5 pl-3 pr-2 text-xs hover:bg-secondary ${
                                isSelected
                                  ? "font-medium text-[#61afef]"
                                  : "text-foreground"
                              }`}
                            >
                              <span>{option.label}</span>
                              {isSelected && <Check className="size-3 text-[#61afef]" />}
                            </DropdownMenuItem>
                          );
                        })}
                      </DropdownMenuSubContent>
                    </DropdownMenuSub>
                  ))
                )}
              </DropdownMenuContent>
            </DropdownMenu>

            {/* Botão redondo com seta pra cima no canto inferior direito */}
            {running && !compacting && <Button type="button" size="icon" variant="destructive" className="size-7.5 cursor-pointer rounded-full" aria-label="Interromper execução" onClick={() => { void onStop?.(); }}><Square className="size-3.5" /></Button>}
            <Button
              type="button"
              size="icon"
              onClick={() => { void handleSend(); }}
              disabled={!text.trim() || disabled || compacting || sending || !currentModelDef}
              aria-label={running ? "Agendar mensagem" : "Enviar mensagem"}
              className={`size-7.5 cursor-pointer rounded-full transition-all ${
                text.trim()
                  ? "bg-[#61afef] text-primary-foreground shadow-sm shadow-[#61afef]/30 hover:bg-[#61afef]/90 active:scale-95"
                  : "cursor-not-allowed bg-secondary text-muted-foreground"
              }`}
            >
              <ArrowUp className="size-3.5 stroke-[2.5]" />
            </Button>
          </div>
        </div>
      </SkillInput></Suspense>
    </div>
  );
}
