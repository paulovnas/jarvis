import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { ArrowUp, Globe, Paperclip, Plus, Send, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/TextInput";
import { Skeleton } from "@/components/ui/skeleton";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { attachmentSchema, uploadFile } from "@/core/attachments";
import { AttachmentPreview } from "./AttachmentPreview";
import { FlowPicker } from "./FlowPicker";
import { ComposerSkeleton } from "@/components/layout/LoadingSkeletons";
import { type ProviderModelGroup, type ModelSelection } from "./ModelPicker";
import { ExecutorModelPicker } from "./ExecutorModelPicker";
import { claudeModels, executionChoice, executionSelection, executorOf, selectModelChoice } from "@/core/executors";
import { useClaudeRuntime } from "@/hooks/use-claude-runtime";
export type { ProviderModelGroup } from "./ModelPicker";
import type { AgentModelsController } from "@/hooks/use-agent-models";
import type { ChatAgentModelsController } from "@/hooks/use-chat-agent-models";
import type { ModelChoice } from "@/core/provider-references";
import { useWorkflowCatalog } from "@/hooks/use-workflow-catalog";
import { flowOptions, flowSelection, type FlowSelection } from "@/core/workflow-catalog";
import { chatAgentModelKey } from "@/core/chat-models";
import { rootRole } from "@/core/workflow";
import { resolveChatModel, type ModelBinding } from "@/core/provider-references";
import { useModelProblemNotice } from "@/hooks/use-provider-references";
import { libraryError } from "@/core/library";
import { mergeDrafts, type ChatDraft, type ChatSnapshot, type MessagePart, type QueuedMessage, type TurnOptions } from "@/core/chat";
import type { PendingQuestion, QuestionResponse } from "@/core/questions";
import { VoiceControls, VoiceSessionPanel } from "@/components/voice/VoiceControls";
import { stopDictation } from "@/hooks/use-voice";
import type { WorkflowSnapshot } from "@/core/workflow";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";
import { QueuedMessagesPanel } from "./QueuedMessagesPanel";
import { ChatBehaviorSettings } from "./ChatBehaviorSettings";
import { Hint } from "@/components/ui/hint";
import type { ModelCatalogRefresh } from "@/core/provider-accounts";
import { defaultReasoning } from "@/core/reasoning";

const SkillInput = lazy(() => import("./SkillInput").then(module => ({ default: module.SkillInput })));

interface ChatComposerProps {
  voiceSnapshot?: ChatSnapshot;
  onVoiceAnswer?: (question: PendingQuestion, response: QuestionResponse) => Promise<boolean>;
  onVoicePause?: (question: PendingQuestion) => Promise<boolean>;
  draftInsertion?: { id: string; text: string };
  onOpenBrowser?: () => void;
  browserBusy?: boolean;
  onOpenHttp?: () => void;
  httpBusy?: boolean;
  agentModels?: AgentModelsController;
  chatModels?: ChatAgentModelsController;
  onSendMessage: (content: string, options: TurnOptions, parts?: MessagePart[]) => Promise<boolean>;
  onStop?: () => Promise<void>;
  running?: boolean;
  compacting?: boolean;
  initialOptions?: TurnOptions;
  disabled?: boolean;
  modelGroups: ProviderModelGroup[];
  modelBindings?: ModelBinding[];
  modelsReady?: boolean;
  onRefreshModels?: () => Promise<ModelCatalogRefresh | null>;
  refreshingModels?: boolean;
  draftKey?: string;
  drafts?: Map<string, ChatDraft>;
  queuedMessages?: QueuedMessage[];
  onRemoveQueued?: (id: string) => Promise<ChatDraft | null>;
  onDeleteQueued?: (id: string) => Promise<boolean>;
  onSendQueuedNow?: (id: string) => Promise<boolean>;
  onReorderQueued?: (ids: string[]) => Promise<boolean>;
  onResumeQueue?: () => Promise<void>;
  workflowSnapshot?: WorkflowSnapshot | null;
}

export function ChatComposer({
  voiceSnapshot,
  onVoiceAnswer,
  onVoicePause,
  draftInsertion,
  onOpenBrowser,
  browserBusy = false,
  onOpenHttp,
  httpBusy = false,
  agentModels,
  chatModels,
  onSendMessage,
  modelGroups,
  modelBindings = [],
  modelsReady = true,
  onRefreshModels,
  refreshingModels = false,
  disabled = false,
  running = false,
  compacting = false,
  onStop,
  initialOptions,
  draftKey,
  drafts,
  queuedMessages = [],
  onRemoveQueued,
  onDeleteQueued,
  onSendQueuedNow,
  onReorderQueued,
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
  const [refreshDialogOpen, setRefreshDialogOpen] = useState(false);
  const attachments = draft.parts?.filter(part => part.type === "attachment") ?? [];
  const removeLocks = useRef(new Set<string>());
  const lastInsertion = useRef<string | null>(null);
  useEffect(() => {
    if (!draftInsertion || draftInsertion.id === lastInsertion.current) return;
    lastInsertion.current = draftInsertion.id;
    const next = mergeDrafts(draftRef.current, { content: draftInsertion.text });
    draftRef.current = next;
    if (draftKey) drafts?.set(draftKey, next);
    updateDraft(next);
    const frame = requestAnimationFrame(() => input.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [draftInsertion, draftKey, drafts]);
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
    removeLocks.current.add(id);
    try {
      const restored = await onRemoveQueued(id);
      if (restored !== null) {
        const current = draftKey && drafts ? drafts.get(draftKey) ?? { content: "" } : draftRef.current;
        setDraft(mergeDrafts(current, restored));
        input.current?.focus();
      }
    } finally { removeLocks.current.delete(id); }
  };
  const [sending, setSending] = useState(false);
  const sendLock = useRef(false);
  const [selection, setSelection] = useState<ModelSelection | null>(executionSelection(initialOptions));
  const [localChoices, setLocalChoices] = useState<Record<string, ModelChoice>>({});
  const catalog = useWorkflowCatalog();
  const [manualBindings, setManualBindings] = useState<ModelBinding[] | null>(null);
  const [choosingModel, setChoosingModel] = useState(false);
  const choosingModelLock = useRef(false);
  const [workflow, setWorkflow] = useState<FlowSelection>(flowSelection(initialOptions));
  const [manualValidation, setManualValidation] = useState(initialOptions?.manualValidation ?? false);
  const [automaticPublication, setAutomaticPublication] = useState<NonNullable<TurnOptions["automaticPublication"]> | null>(initialOptions?.automaticPublication ?? null);
  const [pendingWorkflow, setPendingWorkflow] = useState<FlowSelection | null>(null);
  const selectedFlow = flowOptions(workflow);
  const modelKey = chatAgentModelKey(workflow);
  const manualValidationAvailable = selectedFlow.workflow === "planned"
    || selectedFlow.workflow === "complete"
    || (selectedFlow.workflow === "custom" && Boolean(selectedFlow.customWorkflowId));
  const customFlow = catalog.data?.flows.find(flow => flow.id === selectedFlow.customWorkflowId);
  const selectedCustomAgent = catalog.data?.agents.find(agent => agent.id === selectedFlow.customAgentId);
  const selectedBuiltinAgent = catalog.data?.builtinAgents.find(agent => agent.id === selectedFlow.customAgentId);
  const selectedAgent = selectedCustomAgent ?? selectedBuiltinAgent;
  const githubSelected = selectedFlow.customAgentId === "builtin:github";
  const builtinProfileFlow = githubSelected ? "publication" : selectedFlow.customAgentId === "builtin:video" ? "video" : selectedFlow.customAgentId === "builtin:image_generator" ? "image_generator" : null;
  const customUnavailable = selectedFlow.workflow === "custom" && (selectedFlow.customAgentId
    ? !selectedAgent || selectedAgent.usage === "flow_only"
    : !customFlow);
  const customUnavailableMessage = catalog.error ?? (!catalog.data
    ? selectedFlow.customAgentId ? "Carregando o agente individual…" : "Carregando o fluxo customizado…"
    : selectedFlow.customAgentId ? "Este agente não está mais disponível para uso individual. Escolha outra opção." : "Este fluxo foi removido. Escolha outro fluxo para enviar.");

  const handleSend = async (voiceText?: string): Promise<boolean> => {
    const submitted: ChatDraft = voiceText ? { content: voiceText } : draftRef.current;
    const trimmed = submitted.content.trim() || (submitted.parts?.some(part => part.type === "attachment") ? "Analise os anexos." : "");
    if (modelError) { toast.error("Revise o modelo antes de enviar", { description: modelError }); return false; }
    if (!trimmed || disabled || !selectionReady || customUnavailable || compacting || importing.current || chatModels?.saving || choosingModelLock.current || sendLock.current || !currentModelDef) return false;
    const choice = executionChoice({ executor: executorOf(effectiveSelection), model: currentModelDef.value, reasoning });
    if (choice.executor === "jarvis" && !choice.account) return false;
    sendLock.current = true;
    setSending(true);
    let cleared = false;
    const restoreSubmitted = () => {
      if (voiceText || !cleared) return;
      const current = draftKey && drafts ? drafts.get(draftKey) ?? { content: "" } : draftRef.current;
      setDraft(current.content || current.parts?.length
        ? mergeDrafts(submitted, current)
        : { ...submitted, parts: submitted.parts ? [...submitted.parts] : undefined });
    };
    try {
      const stopping = stopDictation(draftKey ? `chat:${draftKey}` : undefined);
      if (!voiceText) { setDraft({ content: "" }); cleared = true; }
      await stopping;
      const options: TurnOptions = running && initialOptions ? { ...initialOptions, approvalMode: "yolo" } : { ...choice, mode: "build", ...selectedFlow, approvalMode: "yolo" };
      if (manualValidationAvailable && manualValidation && !githubSelected) options.manualValidation = true;
      else delete options.manualValidation;
      if (automaticPublication && !githubSelected) options.automaticPublication = automaticPublication;
      else delete options.automaticPublication;
      const accepted = submitted.parts?.length ? await onSendMessage(trimmed, options, submitted.parts) : await onSendMessage(trimmed, options);
      if (!accepted) restoreSubmitted();
      return accepted;
    } catch (cause) {
      restoreSubmitted();
      toast.error(libraryError(cause, "Não foi possível enviar a mensagem. Seu texto foi mantido."));
      return false;
    } finally { sendLock.current = false; setSending(false); }
  };

  const nativeModels = modelGroups.flatMap((group) => group.models);
  const selectedProfile = builtinProfileFlow
    ? agentModels?.data?.[`${builtinProfileFlow}/${rootRole(builtinProfileFlow)}`]
    : selectedFlow.workflow === "custom" ? undefined : agentModels?.data?.[`${workflow}/${rootRole(selectedFlow.workflow ?? "standard")}`];
  const defaultProfile = agentModels?.data?.["standard/builder"];
  const profile = selectedProfile ?? (!nativeModels.length && executorOf(defaultProfile) !== "jarvis" ? defaultProfile : undefined);
  const savedChoice = chatModels?.data?.[modelKey] ?? localChoices[modelKey];
  const historicalChoice = flowSelection(initialOptions) === workflow ? executionSelection(initialOptions) : null;
  const baseSelection = executionSelection(savedChoice) ?? historicalChoice ?? executionSelection(selectedCustomAgent?.model) ?? executionSelection(profile) ?? selection;
  const boundChoice = !chatModels?.data?.[modelKey] && baseSelection && manualBindings !== modelBindings ? resolveChatModel(modelBindings, draftKey, executionChoice(baseSelection), modelKey) : null;
  const effectiveSelection = running && initialOptions ? executionSelection(initialOptions) : executionSelection(boundChoice) ?? baseSelection;
  const configuredAgents = selectedFlow.customAgentId
    ? savedChoice ? [{ name: selectedAgent?.name ?? "Chat", choice: savedChoice }] : selectedCustomAgent?.model ? [{ name: selectedCustomAgent.name, choice: selectedCustomAgent.model }] : selectedBuiltinAgent && profile ? [{ name: selectedBuiltinAgent.name, choice: profile }] : []
    : selectedFlow.workflow === "custom"
      ? [...(savedChoice ? [{ name: customFlow?.name ?? "Chat", choice: savedChoice }] : []), ...(catalog.data?.agents.flatMap(agent => agent.model && customFlow?.steps.some(step => step.agentId === agent.id) ? [{ name: agent.name, choice: agent.model }] : []) ?? [])]
      : Object.entries({ ...agentModels?.data, ...chatModels?.data, ...localChoices }).flatMap(([key, choice]) => key.startsWith(`${workflow}/`) ? [{ name: key, choice: key === modelKey && savedChoice ? savedChoice : choice }] : []);
  const claudeRequired = executorOf(effectiveSelection) === "claude" || configuredAgents.some(agent => executorOf(agent.choice) === "claude" || executorOf(agent.choice.fallback) === "claude");
  const claude = useClaudeRuntime(claudeRequired);
  const runtimeModels = claudeModels(claude.data);
  const modelsFor = (choice: ModelSelection | null) => executorOf(choice) === "claude" ? runtimeModels : executorOf(choice) === "unavailable" ? [] : nativeModels;
  const availableModels = modelsFor(effectiveSelection);
  const selectionReady = (!chatModels || chatModels.data !== null && !chatModels.error) && (executorOf(effectiveSelection) !== "jarvis" || modelsReady) && (!claudeRequired || Boolean(claude.data) && !claude.loading);
  const currentModelDef =
    availableModels.find((availableModel) => availableModel.value === effectiveSelection?.model) ??
    (effectiveSelection ? undefined : availableModels[0]);
  const invalidSelection = (choice: ModelSelection) => {
    if (executorOf(choice) === "claude" && !claude.data) return false;
    const model = modelsFor(choice).find(item => item.value === choice.model);
    return !model || Boolean(choice.reasoning && !model.reasoningLevels.includes(choice.reasoning));
  };
  const invalidAgent = configuredAgents.find(agent => invalidSelection(executionSelection(agent.choice)!) || Boolean(agent.choice.fallback && invalidSelection(executionSelection(agent.choice.fallback)!)))?.name;
  const claudeProblem = !claudeRequired || claude.loading ? null : claude.error ?? (claude.data?.preferences?.enabled === false ? "Ative o Claude Code em Configurações → Provedores." : claude.data && !claude.data.installed ? "Instale o Claude Code em Configurações → Provedores e atualize o status." : claude.data && !claude.data.authenticated ? "Entre na sua conta com claude auth login e atualize o status do Claude Code." : null);
  const removedExecutor = executorOf(effectiveSelection) === "unavailable" || configuredAgents.some(agent => executorOf(agent.choice) === "unavailable" || executorOf(agent.choice.fallback) === "unavailable");
  const modelError = chatModels?.error ?? (removedExecutor ? "Este executor foi removido. Escolha um provedor e modelo disponíveis para continuar." : null) ?? claudeProblem ?? (!selectionReady ? null : invalidAgent
    ? `O agente ${invalidAgent} usa um modelo indisponível. Selecione um modelo válido para este chat.`
    : effectiveSelection && invalidSelection(effectiveSelection) ? `O modelo ${effectiveSelection.model} está indisponível. Revise o provedor e o modelo deste chat.` : null);
  useModelProblemNotice("Chat", modelError, `chat:${draftKey ?? "new"}`);
  const reasoning =
    currentModelDef?.value === effectiveSelection?.model &&
    effectiveSelection?.reasoning &&
    currentModelDef?.reasoningLevels.includes(effectiveSelection.reasoning)
      ? effectiveSelection.reasoning
      : currentModelDef ? defaultReasoning(currentModelDef) : null;
  const chooseModel = (next: ModelSelection) => {
    const choice = selectModelChoice(savedChoice ?? selectedCustomAgent?.model ?? profile, executionChoice(next), "primary");
    if (choice.fallback && invalidSelection(executionSelection(choice.fallback)!)) choice.fallback = null;
    if (chatModels) { void chatModels.save(modelKey, choice); }
    else {
      const bound = resolveChatModel(modelBindings, draftKey, choice);
      const accept = () => { setLocalChoices(current => ({ ...current, [modelKey]: choice })); setSelection(executionSelection(choice)); };
      if (!draftKey || JSON.stringify(choice) === JSON.stringify(bound)) { accept(); return; }
      if (choosingModelLock.current) return;
      choosingModelLock.current = true; setChoosingModel(true);
      void invoke("clear_chat_model_binding", { conversationId: draftKey, choice }).then(() => { setManualBindings(modelBindings); accept(); }).catch(cause => toast.error(libraryError(cause, "Não foi possível atualizar o modelo do chat."))).finally(() => { choosingModelLock.current = false; setChoosingModel(false); });
    }
  };
  const chooseWorkflow = (next: FlowSelection) => {
    const currentOptions = flowOptions(workflow);
    const currentFlow = currentOptions.workflow;
    const leavingCoordinatedFlow = (["planned", "complete"].includes(currentFlow ?? "")
      || currentFlow === "custom" && !!currentOptions.customWorkflowId && !currentOptions.customAgentId)
      && (next === "standard" || next === "designer" || next === "video" || next === "image_generator" || next.startsWith("agent:"));
    const currentSnapshot = workflowSnapshot;
    const hasWorkflowState = currentSnapshot != null && currentSnapshot.flow === currentFlow
      && (currentSnapshot.agents.some(agent => agent.id !== "main")
        || currentSnapshot.validation?.items.some(item => item.decision === "pending"));
    if (leavingCoordinatedFlow && hasWorkflowState) {
      setPendingWorkflow(next);
      return;
    }
    setWorkflow(next);
    if (next === "standard" || next === "designer" || next === "video" || next === "image_generator" || next.startsWith("agent:")) setManualValidation(false);
  };

  const refreshModels = async () => {
    setRefreshDialogOpen(false);
    if (!onRefreshModels) return;
    try {
      const report = await onRefreshModels();
      if (!report) return;
      if (report.failed.length === 0) {
        toast.success("Modelos atualizados", { description: `${report.refreshed.length} conta(s) consultada(s).` });
      } else if (report.refreshed.length > 0) {
        toast.warning("Atualização parcial dos modelos", { description: `Não foi possível atualizar: ${report.failed.join(", ")}. Os modelos anteriores foram mantidos.` });
      } else {
        toast.error("Não foi possível atualizar os modelos", { description: `Contas: ${report.failed.join(", ")}. Os modelos anteriores foram mantidos.` });
      }
    } catch (error) {
      toast.error("Não foi possível atualizar os modelos", { description: libraryError(error, "Verifique a conexão e tente novamente.") });
    }
  };

  return (
    <div className="w-full">
      {modelError && <p role="alert" className="px-4 py-2 text-xs text-destructive">{modelError}</p>}
      {customUnavailable && <p role="alert" className="px-4 py-2 text-xs text-destructive">{customUnavailableMessage}</p>}
      {queuedMessages.length > 0 && <QueuedMessagesPanel
        messages={queuedMessages}
        running={running}
        compacting={compacting}
        onEdit={removeQueued}
        onDelete={onDeleteQueued}
        onSendNow={onSendQueuedNow}
        onReorder={onReorderQueued}
        onResume={onResumeQueue}
      />}
      <Suspense fallback={<ComposerSkeleton />}><SkillInput ref={input} draft={draft} onChange={setDraft} onFiles={files => { void addFiles(files); }} attachments={(attachments.length > 0 || uploading) && <div className="flex flex-wrap gap-1 px-5 pt-4" aria-label="Anexos da mensagem">{attachments.map(part => <AttachmentPreview key={part.attachment.id} attachment={part.attachment} disabled={disabled || compacting || sending} onRemove={() => { const current = draftRef.current; setDraft({ ...current, parts: current.parts?.filter(item => item.type !== "attachment" || item.attachment.id !== part.attachment.id) }); }} />)}{uploading && <Skeleton className="mb-2 size-20 rounded-lg" role="status" aria-label="Preparando anexos" />}</div>} onSend={() => { void handleSend(); }} disabled={disabled || compacting} compacting={compacting} working={running || compacting}>

        <div className="flex w-full min-w-0 flex-col">
        {draftKey && <div className="px-4"><VoiceSessionPanel target={`chat:${draftKey}`} /></div>}
        {/* Linha de controles inferior no padrão Metis */}
        <div className="composer-controls flex w-full items-center justify-between gap-1 px-3 pb-3 pt-1">
          <DropdownMenu>
            <Hint content="Mais ações"><DropdownMenuTrigger render={<Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="Mais ações"
              className="size-7.5 cursor-pointer rounded-full bg-secondary text-foreground transition-colors hover:bg-accent hover:text-foreground"
            />}><Plus className="stroke-[2.2]" /></DropdownMenuTrigger></Hint>
            <DropdownMenuContent side="top" align="start" className="w-60">
              <DropdownMenuGroup>
                <DropdownMenuItem className="cursor-pointer" disabled={disabled || compacting || uploading || !draftKey} onClick={() => fileInput.current?.click()}><Paperclip />Anexar arquivos</DropdownMenuItem>
                {onOpenBrowser && <DropdownMenuItem className="cursor-pointer" disabled={browserBusy} onClick={onOpenBrowser}><Globe />Nova aba de navegador</DropdownMenuItem>}
                {onOpenHttp && <DropdownMenuItem className="cursor-pointer" disabled={httpBusy} onClick={onOpenHttp}><Send />Nova requisição HTTP</DropdownMenuItem>}
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
          <Input ref={fileInput} type="file" multiple className="hidden" aria-label="Selecionar anexos" accept="image/png,image/jpeg,image/webp,image/gif,image/bmp,.pdf,.docx,.odt,.txt,.md,.csv,.json,.xml,.yaml,.yml,.log,.ts,.tsx,.js,.css,.html,.rs,.py" onChange={event => { const files = Array.from(event.target.files ?? []); event.target.value = ""; void addFiles(files); }} />

          {/* Canto inferior direito: seletor de modo/agente, seletor de modelo e botão redondo de envio */}
          {draftKey && <VoiceControls target={`chat:${draftKey}`} snapshot={voiceSnapshot} onAnswerQuestion={onVoiceAnswer} onPauseQuestion={onVoicePause} disabled={disabled || compacting || sending || !selectionReady} onDictation={text => { setDraft(mergeDrafts(draftRef.current, { content: text })); input.current?.focus(); }} onMessage={text => handleSend(text)} />}
          <div className="composer-options flex flex-1 items-center gap-0.5">
            <ChatBehaviorSettings manualAvailable={manualValidationAvailable} manualValidation={manualValidation} onManualChange={setManualValidation} publication={githubSelected ? null : automaticPublication} onPublicationChange={setAutomaticPublication} disabled={running || sending || compacting} githubSelected={githubSelected} />
            <FlowPicker customFlows={catalog.data?.flows} customAgents={catalog.data?.agents} builtinAgents={catalog.data?.builtinAgents} value={workflow} onChange={chooseWorkflow} disabled={running || sending || compacting} />

            <ExecutorModelPicker modelGroups={modelGroups} selection={currentModelDef && !modelError ? { executor: executorOf(effectiveSelection), model: currentModelDef.value, reasoning } : effectiveSelection} onSelect={chooseModel} nativeDisabled={!modelsReady} disabled={running || sending || compacting || choosingModel || chatModels?.saving} invalid={Boolean(modelError)} showProviderIdentity onRefresh={onRefreshModels ? () => setRefreshDialogOpen(true) : undefined} refreshing={refreshingModels} />

            {/* Botão redondo com seta pra cima no canto inferior direito */}
            {running && !compacting && <Hint content="Interromper execução"><Button type="button" size="icon" variant="destructive" className="size-7.5 cursor-pointer rounded-full" aria-label="Interromper execução" onClick={() => { void onStop?.(); }}><Square className="size-3.5" /></Button></Hint>}
            <Hint content={running ? "Agendar mensagem" : "Enviar mensagem"}><Button
              type="button"
              size="icon"
              onClick={() => { void handleSend(); }}
              disabled={(!text.trim() && !attachments.length) || disabled || !selectionReady || Boolean(modelError) || customUnavailable || compacting || choosingModel || sending || uploading || !currentModelDef}
              aria-label={running ? "Agendar mensagem" : "Enviar mensagem"}
              className={`size-7.5 cursor-pointer rounded-full transition-all ${
                text.trim() || attachments.length
                  ? "bg-[#61afef] text-primary-foreground shadow-sm shadow-[#61afef]/30 hover:bg-[#61afef]/90 active:scale-95"
                  : "cursor-not-allowed bg-secondary text-muted-foreground"
              }`}
            >
              <ArrowUp className="size-3.5 stroke-[2.5]" />
            </Button></Hint>
          </div>
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
            <AlertDialogAction data-confirm-action className="cursor-pointer" onClick={() => { if (pendingWorkflow) { setWorkflow(pendingWorkflow); if (pendingWorkflow === "standard" || pendingWorkflow === "designer" || pendingWorkflow === "video" || pendingWorkflow === "image_generator" || pendingWorkflow.startsWith("agent:")) setManualValidation(false); } setPendingWorkflow(null); }}>Trocar fluxo</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog open={refreshDialogOpen} onOpenChange={setRefreshDialogOpen}>
        <AlertDialogContent className="dark">
          <AlertDialogHeader>
            <AlertDialogTitle>Atualizar modelos disponíveis?</AlertDialogTitle>
            <AlertDialogDescription>O Jarvis consultará os catálogos das contas OpenAI/Codex e Antigravity conectadas. Esse processo pode demorar alguns minutos. Não será necessário entrar nas contas novamente; se uma consulta falhar, os modelos atuais serão mantidos.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel className="cursor-pointer">Cancelar</AlertDialogCancel>
            <AlertDialogAction data-confirm-action className="cursor-pointer" onClick={() => { void refreshModels(); }}>Atualizar modelos</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
