import { lazy, Suspense, useRef, useState, type ReactNode } from "react";
import { ArrowUp, Plus, Square, ListOrdered, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/TextInput";
import { Skeleton } from "@/components/ui/skeleton";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { attachmentSchema, uploadFile } from "@/core/attachments";
import { AttachmentPreview } from "./AttachmentPreview";
import { FlowPicker } from "./FlowPicker";
import { ComposerSkeleton } from "@/components/layout/LoadingSkeletons";
import { MessageContent } from "./MessageContent";
import { ModelPicker, type ProviderModelGroup, type ModelSelection } from "./ModelPicker";
export type { ProviderModelGroup } from "./ModelPicker";
import type { AgentModelsController } from "@/hooks/use-agent-models";
import { useWorkflowCatalog } from "@/hooks/use-workflow-catalog";
import { flowOptions, flowSelection, type FlowSelection } from "@/core/workflow-catalog";
import { rootRole } from "@/core/workflow";
import { resolveChatModel, type ModelBinding } from "@/core/provider-references";
import { useModelProblemNotice } from "@/hooks/use-provider-references";
import { libraryError } from "@/core/library";
import { mergeDrafts, type ChatDraft, type MessagePart, type QueuedMessage, type TurnOptions } from "@/core/chat";
import type { WorkflowSnapshot } from "@/core/workflow";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";

const SkillInput = lazy(() => import("./SkillInput").then(module => ({ default: module.SkillInput })));

interface ChatComposerProps {
  terminalLauncher?: ReactNode;
  agentModels?: AgentModelsController;
  onSendMessage: (content: string, options: TurnOptions, parts?: MessagePart[]) => Promise<boolean>;
  onStop?: () => Promise<void>;
  running?: boolean;
  compacting?: boolean;
  initialOptions?: TurnOptions;
  disabled?: boolean;
  modelGroups: ProviderModelGroup[];
  modelBindings?: ModelBinding[];
  modelsReady?: boolean;
  draftKey?: string;
  drafts?: Map<string, ChatDraft>;
  queuedMessages?: QueuedMessage[];
  onRemoveQueued?: (id: string) => Promise<ChatDraft | null>;
  onResumeQueue?: () => Promise<void>;
  workflowSnapshot?: WorkflowSnapshot | null;
}

export function ChatComposer({
  terminalLauncher,
  agentModels,
  onSendMessage,
  modelGroups,
  modelBindings = [],
  modelsReady = true,
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
  workflowSnapshot,
}: ChatComposerProps) {
  const [draft, updateDraft] = useState<ChatDraft>(() => draftKey ? drafts?.get(draftKey) ?? { content: "" } : { content: "" });
  const text = draft.content;
  const draftRef = useRef(draft);
  const input = useRef<{ focus: () => void }>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const importing = useRef(false);
  const [uploading, setUploading] = useState(false);
  const attachments = draft.parts?.filter(part => part.type === "attachment") ?? [];
  const [removing, setRemoving] = useState<string[]>([]);
  const removeLocks = useRef(new Set<string>());
  const [resuming, setResuming] = useState(false);
  const setDraft = (value: ChatDraft) => {
    draftRef.current = value;
    if (draftKey) { if (value.content || value.parts?.length) drafts?.set(draftKey, value); else drafts?.delete(draftKey); }
    updateDraft(value);
  };
  async function addFiles(files: File[]) {
    if (!draftKey || disabled || compacting || importing.current || !files.length) return;
    const existing = draftRef.current.parts?.filter(part => part.type === "attachment") ?? [];
    if (files.length + existing.length > 8 || files.reduce((sum, file) => sum + file.size, existing.reduce((sum, part) => sum + part.attachment.size, 0)) > 50 * 1024 * 1024) { toast.error("Anexe até 8 arquivos, somando no máximo 50 MB."); return; }
    importing.current = true; setUploading(true);
    try {
      const uploads = await Promise.all(files.map(uploadFile));
      const imported = attachmentSchema.array().parse(await invoke("import_chat_attachments", { conversationId: draftKey, uploads }));
      const current = drafts?.get(draftKey) ?? draftRef.current;
      setDraft({ ...current, parts: [...(current.parts ?? [{ type: "text", text: current.content }]), ...imported.map(attachment => ({ type: "attachment" as const, attachment }))] });
    } catch (cause) { toast.error(cause instanceof Error ? cause.message : typeof cause === "object" && cause && "message" in cause ? String(cause.message) : "Não foi possível anexar os arquivos."); }
    finally { importing.current = false; setUploading(false); }
  }
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
  const catalog = useWorkflowCatalog();
  const [manualBindings, setManualBindings] = useState<ModelBinding[] | null>(null);
  const [choosingModel, setChoosingModel] = useState(false);
  const choosingModelLock = useRef(false);
  const [workflow, setWorkflow] = useState<FlowSelection>(flowSelection(initialOptions));
  const [pendingWorkflow, setPendingWorkflow] = useState<FlowSelection | null>(null);
  const selectedFlow = flowOptions(workflow);
  const customFlow = catalog.data?.flows.find(flow => flow.id === selectedFlow.customWorkflowId);
  const selectedCustomAgent = catalog.data?.agents.find(agent => agent.id === selectedFlow.customAgentId);
  const customUnavailable = selectedFlow.workflow === "custom" && (selectedFlow.customAgentId
    ? !selectedCustomAgent || selectedCustomAgent.usage === "flow_only"
    : !customFlow);
  const customUnavailableMessage = catalog.error ?? (!catalog.data
    ? selectedFlow.customAgentId ? "Carregando o agente individual…" : "Carregando o fluxo customizado…"
    : selectedFlow.customAgentId ? "Este agente não está mais disponível para uso individual. Escolha outra opção." : "Este fluxo foi removido. Escolha outro fluxo para enviar.");

  const handleSend = async () => {
    const submitted = draftRef.current;
    const trimmed = submitted.content.trim() || (submitted.parts?.some(part => part.type === "attachment") ? "Analise os anexos." : "");
    if (modelError) { toast.error("Revise o modelo antes de enviar", { description: modelError }); return; }
    if (!trimmed || disabled || !modelsReady || customUnavailable || compacting || importing.current || agentModels?.saving || choosingModelLock.current || sendLock.current || !currentModelDef) return;
    const separator = currentModelDef.value.indexOf("/");
    if (separator < 1) return;
    sendLock.current = true;
    setSending(true);
    setDraft({ content: "" });
    const restoreSubmitted = () => {
      const current = draftKey && drafts ? drafts.get(draftKey) ?? { content: "" } : draftRef.current;
      setDraft(current.content || current.parts?.length
        ? mergeDrafts(submitted, current)
        : { ...submitted, parts: submitted.parts ? [...submitted.parts] : undefined });
    };
    try {
      const options: TurnOptions = running && initialOptions ? { ...initialOptions, approvalMode: "yolo" } : { account: currentModelDef.value.slice(0, separator), model: currentModelDef.value.slice(separator + 1), reasoning, mode: "build", ...selectedFlow, approvalMode: "yolo" };
      const accepted = submitted.parts?.length ? await onSendMessage(trimmed, options, submitted.parts) : await onSendMessage(trimmed, options);
      if (!accepted) restoreSubmitted();
    } catch (cause) {
      restoreSubmitted();
      toast.error(libraryError(cause, "Não foi possível enviar a mensagem. Seu texto foi mantido."));
    } finally { sendLock.current = false; setSending(false); }
  };

  const availableModels = modelGroups.flatMap((group) => group.models);
  const profile = selectedFlow.workflow === "custom" ? undefined : agentModels?.data?.[`${workflow}/${rootRole(selectedFlow.workflow ?? "standard")}`];
  const customAgentSelection = selectedCustomAgent?.model ? { model: `${selectedCustomAgent.model.account}/${selectedCustomAgent.model.model}`, reasoning: selectedCustomAgent.model.reasoning } : null;
  const separator = selection?.model.indexOf("/") ?? -1;
  const boundChoice = selection && separator > 0 && manualBindings !== modelBindings ? resolveChatModel(modelBindings, draftKey, { account: selection.model.slice(0, separator), model: selection.model.slice(separator + 1), reasoning: selection.reasoning }) : null;
  const effectiveSelection = running && initialOptions ? { model: `${initialOptions.account}/${initialOptions.model}`, reasoning: initialOptions.reasoning } : customAgentSelection ?? (profile ? { model: `${profile.account}/${profile.model}`, reasoning: profile.reasoning } : boundChoice ? { model: `${boundChoice.account}/${boundChoice.model}`, reasoning: boundChoice.reasoning } : selection);
  const currentModelDef =
    availableModels.find((availableModel) => availableModel.value === effectiveSelection?.model) ??
    (effectiveSelection ? undefined : availableModels[0]);
  const invalidSelection = (choice: ModelSelection) => {
    const model = availableModels.find(item => item.value === choice.model);
    return !model || Boolean(choice.reasoning && !model.reasoningLevels.includes(choice.reasoning));
  };
  const invalidAgent = selectedFlow.customAgentId
    ? selectedCustomAgent?.model && invalidSelection({ model: `${selectedCustomAgent.model.account}/${selectedCustomAgent.model.model}`, reasoning: selectedCustomAgent.model.reasoning }) ? selectedCustomAgent.name : undefined
    : selectedFlow.workflow === "custom"
      ? catalog.data?.agents.find(agent => agent.model && customFlow?.steps.some(step => step.agentId === agent.id) && invalidSelection({ model: `${agent.model.account}/${agent.model.model}`, reasoning: agent.model.reasoning }))?.name
    : Object.entries(agentModels?.data ?? {}).find(([key, choice]) => key.startsWith(`${workflow}/`) && invalidSelection({ model: `${choice.account}/${choice.model}`, reasoning: choice.reasoning }))?.[0];
  const modelError = !modelsReady ? null : invalidAgent
    ? `O agente ${invalidAgent} usa um modelo indisponível. Revise o modelo em Configurações → Workflow.`
    : effectiveSelection && invalidSelection(effectiveSelection) ? `O modelo ${effectiveSelection.model} está indisponível. Escolha outro provedor e modelo para este chat.` : null;
  useModelProblemNotice("Chat", modelError, `chat:${draftKey ?? "new"}`);
  const reasoning =
    currentModelDef?.value === effectiveSelection?.model &&
    effectiveSelection?.reasoning &&
    currentModelDef?.reasoningLevels.includes(effectiveSelection.reasoning)
      ? effectiveSelection.reasoning
      : currentModelDef?.defaultReasoningLevel ?? currentModelDef?.reasoningLevels[0] ?? null;
  const chooseModel = (next: ModelSelection) => {
    if (selectedCustomAgent?.model) return;
    if (agentModels && selectedFlow.workflow !== "custom") { void agentModels.save(selectedFlow.workflow ?? "standard", rootRole(selectedFlow.workflow ?? "standard"), { account: next.model.slice(0, next.model.indexOf("/")), model: next.model.slice(next.model.indexOf("/") + 1), reasoning: next.reasoning }); }
    else {
      const split = next.model.indexOf("/");
      const choice = { account: next.model.slice(0, split), model: next.model.slice(split + 1), reasoning: next.reasoning };
      const bound = resolveChatModel(modelBindings, draftKey, choice);
      if (!draftKey || JSON.stringify(choice) === JSON.stringify(bound)) { setSelection(next); return; }
      if (choosingModelLock.current) return;
      choosingModelLock.current = true; setChoosingModel(true);
      void invoke("clear_chat_model_binding", { conversationId: draftKey, choice }).then(() => { setManualBindings(modelBindings); setSelection(next); }).catch(cause => toast.error(libraryError(cause, "Não foi possível atualizar o modelo do chat."))).finally(() => { choosingModelLock.current = false; setChoosingModel(false); });
    }
  };
  const chooseWorkflow = (next: FlowSelection) => {
    const currentFlow = flowOptions(workflow).workflow;
    const leavingCoordinatedFlow = ["planned", "complete"].includes(currentFlow ?? "")
      && (next === "standard" || next === "designer" || next.startsWith("agent:"));
    const currentSnapshot = workflowSnapshot;
    const hasWorkflowState = currentSnapshot != null && currentSnapshot.flow === currentFlow
      && (currentSnapshot.agents.some(agent => agent.id !== "main")
        || currentSnapshot.validation?.items.some(item => item.decision === "pending"));
    if (leavingCoordinatedFlow && hasWorkflowState) {
      setPendingWorkflow(next);
      return;
    }
    setWorkflow(next);
  };

  return (
    <div className="w-full">
      {modelError && <p role="alert" className="px-4 py-2 text-xs text-destructive">{modelError}</p>}
      {customUnavailable && <p role="alert" className="px-4 py-2 text-xs text-destructive">{customUnavailableMessage}</p>}
      {queuedMessages.length > 0 && <section aria-label="Mensagens agendadas" className="mx-3 rounded-t-xl border border-b-0 border-border bg-card px-3 py-2">
        <div className="mb-1 flex items-center gap-2 text-[11px] text-muted-foreground"><ListOrdered className="size-3.5" /><span>{running ? "Após a resposta atual" : "Fila pausada"} · {queuedMessages.length}</span>
          {!running && <Button variant="ghost" size="sm" className="ml-auto h-6 cursor-pointer text-[11px]" disabled={resuming || compacting} onClick={() => { setResuming(true); void onResumeQueue?.().finally(() => setResuming(false)); }}>Continuar fila</Button>}
        </div>
        <div className="max-h-36 overflow-y-auto">{queuedMessages.map((message, index) => <div key={message.id} className="flex items-center gap-2 border-t border-border/50 py-1.5 text-xs">
          <span className="text-muted-foreground tabular-nums">{index + 1}</span><p className="min-w-0 flex-1 truncate" title={message.content}><MessageContent content={message.content} parts={message.parts} /></p>
          <Button variant="ghost" size="icon" className="size-6 shrink-0 cursor-pointer text-muted-foreground" aria-label={`Retirar mensagem ${index + 1} e editar`} title="Cancelar envio e devolver ao campo de texto" disabled={compacting || removing.includes(message.id)} onClick={() => { void removeQueued(message.id); }}><X className="size-3.5" /></Button>
        </div>)}</div>
      </section>}
      <Suspense fallback={<ComposerSkeleton />}><SkillInput ref={input} draft={draft} onChange={setDraft} onFiles={files => { void addFiles(files); }} attachments={(attachments.length > 0 || uploading) && <div className="flex flex-wrap gap-1 px-5 pt-4" aria-label="Anexos da mensagem">{attachments.map(part => <AttachmentPreview key={part.attachment.id} attachment={part.attachment} disabled={disabled || compacting || sending} onRemove={() => { const current = draftRef.current; setDraft({ ...current, parts: current.parts?.filter(item => item.type !== "attachment" || item.attachment.id !== part.attachment.id) }); }} />)}{uploading && <Skeleton className="mb-2 size-20 rounded-lg" role="status" aria-label="Preparando anexos" />}</div>} onSend={() => { void handleSend(); }} disabled={disabled || compacting} compacting={compacting} working={running || compacting}>

        {/* Linha de controles inferior no padrão Metis */}
        <div className="composer-controls flex w-full items-center justify-between gap-1 px-3 pb-3 pt-1">
          {/* Canto inferior esquerdo: botão de anexo com ícone plus */}
          <Button
            type="button"
            variant="ghost"
            size="icon"
            disabled={disabled || compacting || uploading || !draftKey}
            onClick={() => fileInput.current?.click()}
            title="Anexar imagens ou documentos"
            aria-label="Adicionar anexo"
            className="size-7.5 cursor-pointer rounded-full bg-secondary text-foreground transition-colors hover:bg-accent hover:text-foreground"
          >
            <Plus className="size-3.5 stroke-[2.2]" />
          </Button>
          {terminalLauncher}
          <Input ref={fileInput} type="file" multiple className="hidden" aria-label="Selecionar anexos" accept="image/png,image/jpeg,image/webp,image/gif,image/bmp,.pdf,.docx,.odt,.txt,.md,.csv,.json,.xml,.yaml,.yml,.log,.ts,.tsx,.js,.css,.html,.rs,.py" onChange={event => { const files = Array.from(event.target.files ?? []); event.target.value = ""; void addFiles(files); }} />

          {/* Canto inferior direito: seletor de modo/agente, seletor de modelo e botão redondo de envio */}
          <div className="composer-options flex flex-1 items-center gap-0.5">
            <FlowPicker customFlows={catalog.data?.flows} customAgents={catalog.data?.agents} value={workflow} onChange={chooseWorkflow} disabled={running || sending || compacting} />

            <ModelPicker modelGroups={modelGroups} selection={currentModelDef && !modelError ? { model: currentModelDef.value, reasoning } : effectiveSelection} onSelect={chooseModel} disabled={!modelsReady || running || sending || compacting || choosingModel || agentModels?.saving || Boolean(selectedCustomAgent?.model)} />

            {/* Botão redondo com seta pra cima no canto inferior direito */}
            {running && !compacting && <Button type="button" size="icon" variant="destructive" className="size-7.5 cursor-pointer rounded-full" aria-label="Interromper execução" onClick={() => { void onStop?.(); }}><Square className="size-3.5" /></Button>}
            <Button
              type="button"
              size="icon"
              onClick={() => { void handleSend(); }}
              disabled={(!text.trim() && !attachments.length) || disabled || !modelsReady || Boolean(modelError) || customUnavailable || compacting || choosingModel || sending || uploading || !currentModelDef}
              aria-label={running ? "Agendar mensagem" : "Enviar mensagem"}
              className={`size-7.5 cursor-pointer rounded-full transition-all ${
                text.trim() || attachments.length
                  ? "bg-[#61afef] text-primary-foreground shadow-sm shadow-[#61afef]/30 hover:bg-[#61afef]/90 active:scale-95"
                  : "cursor-not-allowed bg-secondary text-muted-foreground"
              }`}
            >
              <ArrowUp className="size-3.5 stroke-[2.5]" />
            </Button>
          </div>
        </div>
      </SkillInput></Suspense>
      <AlertDialog open={pendingWorkflow !== null} onOpenChange={open => { if (!open) setPendingWorkflow(null); }}>
        <AlertDialogContent className="dark">
          <AlertDialogHeader>
            <AlertDialogTitle>Trocar para um agente direto?</AlertDialogTitle>
            <AlertDialogDescription>Ao iniciar a próxima mensagem, o histórico visual dos subagentes e as validações pendentes deste fluxo serão substituídos. Confirme somente se deseja encerrar este acompanhamento.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel className="cursor-pointer">Manter fluxo atual</AlertDialogCancel>
            <AlertDialogAction data-confirm-action className="cursor-pointer" onClick={() => { if (pendingWorkflow) setWorkflow(pendingWorkflow); setPendingWorkflow(null); }}>Trocar fluxo</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
