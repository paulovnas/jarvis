import { useCallback, useEffect, useImperativeHandle, useRef, useState, type Ref } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowUp, Check, Eraser, MessageCircle, Square, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { Hint } from "@/components/ui/hint";
import { LazyChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import { CompanionQuestion, type CompanionQuestionContext } from "./CompanionQuestion";
import { ModelPicker, type ModelSelection, type ProviderModelGroup } from "@/components/chat/ModelPicker";
import { executionChoice, executionSelection, executorOf, selectModelChoice } from "@/core/executors";
import type { ModelChoice } from "@/core/provider-references";
import { flowSelection } from "@/core/workflow-catalog";
import { chatAgentModelKey } from "@/core/chat-models";
import { companionChatSchema, companionConversationsSchema, companionModelsSchema, type CompanionChat, type CompanionConversation } from "@/core/companion";
import type { PendingQuestion, QuestionDraft, QuestionResponse } from "@/core/questions";
import { libraryError } from "@/core/library";

const errorMessage = (cause: unknown) => typeof cause === "string" ? cause : libraryError(cause, "Não foi possível conversar com o Jarvito.");
export interface CompanionChatHandle {
  startGeneral: () => void;
  openGeneral: (conversationId: string, revision?: number) => Promise<boolean>;
}

/** A thin view of the same native conversations and agent runtime used by Jarvis. */
export function CompanionChatPane({ active = true, externalQuestions = false, onQuestionChange, onView, onSend, ref }: {
  active?: boolean;
  externalQuestions?: boolean;
  onQuestionChange?: (context: CompanionQuestionContext | null) => void;
  onView?: (conversationId: string, revision: number) => void;
  onSend?: () => void;
  ref?: Ref<CompanionChatHandle>;
}) {
  const [selected, setSelected] = useState("global");
  const [conversations, setConversations] = useState<CompanionConversation[]>([]);
  const [chat, setChat] = useState<CompanionChat | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [preservedRequest, setPreservedRequest] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [models, setModels] = useState<ProviderModelGroup[]>([]);
  const [modelsLoaded, setModelsLoaded] = useState(false);
  const [modelOverride, setModelOverride] = useState<{ conversation: string; selection: ModelSelection; choice?: ModelChoice } | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [questionDrafts] = useState(() => new Map<string, QuestionDraft>());
  const scroll = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);
  const currentConversation = useRef<string | null>(null);
  const globalConversation = useRef<string | null>(null);
  const drafts = useRef(new Map<string, string>());
  const draftKey = selected === "global" ? chat?.global ? chat.conversationId : "global" : selected;
  const applyChat = useCallback((next: CompanionChat) => {
    if (next.global) {
      globalConversation.current = next.conversationId;
      const pending = drafts.current.get("global");
      if (pending !== undefined) { drafts.current.set(next.conversationId, pending); drafts.current.delete("global"); }
    }
    setChat(current => current?.conversationId === next.conversationId && current.chat.revision > next.chat.revision ? current : next);
  }, []);

  useEffect(() => {
    if (!active) return;
    let alive = true;
    let version = 0;
    let timer: number | undefined;
    let inFlight = false;
    let dirty = false;
    stickToBottom.current = true; currentConversation.current = null;
    const refresh = async () => {
      if (inFlight) { dirty = true; return; }
      inFlight = true;
      const request = ++version;
      try {
        const result = companionChatSchema.parse(await invoke("get_companion_chat", selected === "global" ? undefined : { conversationId: selected }));
        if (alive && request === version) { currentConversation.current = result.conversationId; applyChat(result); setError(null); }
      } catch (cause) { if (alive && request === version) setError(errorMessage(cause)); }
      finally {
        inFlight = false;
        if (alive) { setLoading(false); if (dirty) { dirty = false; schedule(); } }
      }
    };
    const schedule = () => {
      if (!alive || timer !== undefined) return;
      timer = window.setTimeout(() => { timer = undefined; void refresh(); }, 100);
    };
    const subscription = listen<{ conversationId: string }>("companion:chat_changed", event => {
      if (selected === event.payload.conversationId || selected === "global" && (!currentConversation.current || currentConversation.current === event.payload.conversationId)) schedule();
    });
    const modelSubscription = listen<{ conversationId: string }>("chat-agent-models:changed", event => {
      if (currentConversation.current === event.payload.conversationId) schedule();
    });
    const refreshModels = () => {
      void invoke("get_companion_models").then(value => { if (alive) { setModels(companionModelsSchema.parse(value)); setModelsLoaded(true); } }).catch(cause => { if (alive) setError(errorMessage(cause)); });
    };
    const providerSubscriptions = Promise.all(["system:changed", "provider-model-bindings:changed"].map(name => listen(name, () => { if (alive) { refreshModels(); schedule(); } })));
    void subscription.then(() => { if (alive) void refresh(); }).catch(cause => { if (alive) { setError(errorMessage(cause)); setLoading(false); } });
    void invoke("get_companion_conversations").then(value => { if (alive) setConversations(companionConversationsSchema.parse(value)); }).catch(cause => { if (alive) setError(errorMessage(cause)); });
    refreshModels();
    return () => { alive = false; ++version; window.clearTimeout(timer); void subscription.then(stop => stop()).catch(() => {}); void modelSubscription.then(stop => stop()).catch(() => {}); void providerSubscriptions.then(stops => stops.forEach(stop => stop())).catch(() => {}); };
  }, [active, selected, attempt, applyChat]);

  const selectConversation = (value: string) => {
    setDraft(drafts.current.get(value === "global" ? globalConversation.current ?? "global" : value) ?? "");
    setLoading(true); setError(null); setActionError(null); setPreservedRequest(null); setSelected(value);
  };

  const chooseConversation = (value: string) => {
    if (value === selected || busy) return;
    setChat(null); selectConversation(value);
  };
  useImperativeHandle(ref, () => ({
    startGeneral: () => chooseConversation("global"),
    openGeneral: async (conversationId, revision) => {
      if (busy) return false;
      const next = companionChatSchema.parse(await invoke("get_companion_chat"));
      if (!next.global || next.conversationId !== conversationId || revision !== undefined && next.chat.revision < revision) throw new Error("A resposta desta conversa ainda não está disponível. Tente novamente.");
      setSelected("global"); setLoading(false); setError(null); setActionError(null); setPreservedRequest(null);
      setDraft(drafts.current.get(next.conversationId) ?? drafts.current.get("global") ?? "");
      currentConversation.current = next.conversationId; applyChat(next); stickToBottom.current = true;
      return true;
    },
  }));

  useEffect(() => {
    const viewport = scroll.current?.querySelector<HTMLElement>("[data-slot=scroll-area-viewport]");
    if (viewport && stickToBottom.current) viewport.scrollTop = viewport.scrollHeight;
  }, [chat]);

  const send = async (): Promise<boolean> => {
    const content = draft.trim();
    if (!content || busy || loading) return false;
    if (modelInvalid) { setActionError("O modelo deste chat não está disponível. Selecione outro modelo."); return false; }
    setBusy(true); setActionError(null); setPreservedRequest(null);
    try {
      const selectedOptions = modelOverride?.conversation === selected ? {
        ...(chat?.options ?? { account: "", model: "", reasoning: null, mode: "build" as const, workflow: "standard" as const, approvalMode: "yolo" as const }),
        ...executionChoice(modelOverride.selection),
      } : undefined;
      const result = companionChatSchema.parse(await invoke("send_companion_message", { conversationId: selected === "global" ? null : selected, content, ...(selectedOptions ? { options: selectedOptions } : {}) }));
      applyChat(result); onSend?.(); setDraft(current => {
        if (current.trim() !== content) return current;
        drafts.current.delete(draftKey); return "";
      }); stickToBottom.current = true;
      setModelOverride(null);
      return true;
    } catch (cause) { setActionError(errorMessage(cause)); return false; }
    finally { setBusy(false); }
  };
  const stop = async () => {
    if (!chat || busy) return;
    setBusy(true); setActionError(null);
    try { applyChat(companionChatSchema.parse(await invoke("stop_companion_chat", { conversationId: chat.conversationId }))); }
    catch (cause) { setActionError(errorMessage(cause)); }
    finally { setBusy(false); }
  };
  const clear = async () => {
    if (selected !== "global" || !chat?.global || busy || loading || clearBlocked) return;
    setBusy(true); setActionError(null);
    try {
      const result = companionChatSchema.parse(await invoke("clear_companion_chat"));
      if (!result.global || result.conversationId !== chat.conversationId) throw new Error("A resposta não corresponde à conversa geral do Jarvito.");
      applyChat(result);
      setDraft(current => {
        if (current !== draft) return current;
        drafts.current.delete(draftKey); drafts.current.delete("global"); return "";
      });
      setPreservedRequest(null); setError(null);
      if (selectedModel) setModelOverride({ conversation: selected, selection: selectedModel, choice: selectedChoice ?? undefined });
      stickToBottom.current = true;
    } catch (cause) { setActionError(errorMessage(cause)); }
    finally { setBusy(false); }
  };
  const confirm = async (confirmed: boolean): Promise<boolean> => {
    if (!chat?.proposal || busy) return false;
    const proposal = chat.proposal;
    setBusy(true); setActionError(null);
    try {
      const result = companionChatSchema.parse(await invoke("confirm_companion_project", { proposalId: proposal.id, confirmed }));
      applyChat(result);
      if (confirmed && !result.global) selectConversation(result.conversationId);
      return true;
    } catch (cause) {
      setActionError(`${errorMessage(cause)} Seu pedido foi preservado para você continuar.`);
      setPreservedRequest(proposal.message);
      setDraft(current => {
        if (current.trim()) return current;
        drafts.current.set(draftKey, proposal.message); return proposal.message;
      });
      try { applyChat(companionChatSchema.parse(await invoke("get_companion_chat", selected === "global" ? undefined : { conversationId: selected }))); }
      catch (refreshCause) { setError(errorMessage(refreshCause)); }
      return false;
    }
    finally { setBusy(false); }
  };
  const answer = async (question: PendingQuestion, response: QuestionResponse) => {
    if (!chat) return false;
    try {
      return await invoke("companion_answer_question", { conversationId: chat.conversationId, agentId: null, turnId: question.turnId, toolId: question.toolId, response }) !== false;
    } catch (cause) { setActionError(errorMessage(cause)); return false; }
  };
  const pause = async (question: PendingQuestion) => {
    if (!chat) return false;
    try {
      return await invoke("companion_pause_question", { conversationId: chat.conversationId, agentId: null, turnId: question.turnId, toolId: question.toolId }) !== false;
    } catch (cause) { setActionError(errorMessage(cause)); return false; }
  };
  const running = Boolean(chat?.chat.activeTurnId);
  const clearBlocked = running || Boolean(chat?.chat.queuedMessages?.length || chat?.chat.compacting || chat?.chat.context?.compacting);
  const selectedModel = modelOverride?.conversation === selected ? modelOverride.selection : executionSelection(chat?.options);
  const selectedChoice = modelOverride?.conversation === selected ? modelOverride.choice ?? chat?.options?.modelSelection : chat?.options?.modelSelection;
  const invalidSelection = (selection: ModelSelection | null) => {
    const definition = models.filter(group => executorOf(group) === executorOf(selection)).flatMap(group => group.models).find(model => model.value === selection?.model);
    return Boolean(selection && (!definition || selection.reasoning && !definition.reasoningLevels.includes(selection.reasoning)));
  };
  const modelInvalid = modelsLoaded && (invalidSelection(selectedModel) || invalidSelection(executionSelection(selectedChoice?.fallback)));
  const chooseModel = async (selection: ModelSelection) => {
    if (!chat || busy || loading || running) return;
    setBusy(true); setActionError(null);
    try {
      const choice = selectModelChoice(selectedChoice, executionChoice(selection), "primary");
      if (choice.fallback && invalidSelection(executionSelection(choice.fallback))) choice.fallback = null;
      await invoke("set_chat_agent_model", { conversationId: chat.conversationId, key: chatAgentModelKey(flowSelection(chat.options ?? undefined)), choice });
      setModelOverride({ conversation: selected, selection: executionSelection(choice)!, choice });
    } catch (cause) { setActionError(errorMessage(cause)); }
    finally { setBusy(false); }
  };
  const selectedTitle = selected === "global" ? "Conversar com Jarvito" : conversations.find(item => item.id === selected)?.title ?? chat?.projectName ?? "Conversa do projeto";

  useEffect(() => {
    if (!onQuestionChange) return;
    onQuestionChange(active && chat && (chat.chat.pendingQuestion || chat.chat.pendingApproval || chat.chat.pendingAuthoring) ? {
      conversationId: chat.conversationId, agentId: null, title: selectedTitle,
      projectName: chat.projectName ?? "Conversa com Jarvito", request: chat.chat.pendingQuestion,
      requiresConversation: Boolean(chat.chat.pendingApproval || chat.chat.pendingAuthoring),
    } : null);
  }, [active, chat, selectedTitle, onQuestionChange]);
  useEffect(() => {
    if (active && chat && !chat.chat.activeTurnId && !chat.chat.compacting && !chat.chat.pendingQuestion && !chat.chat.pendingApproval && !chat.chat.pendingAuthoring) onView?.(chat.conversationId, chat.chat.revision);
  }, [active, chat, onView]);

  if (active && chat && (chat.chat.pendingQuestion || chat.chat.pendingApproval || chat.chat.pendingAuthoring) && !externalQuestions) return <CompanionQuestion context={{
    conversationId: chat.conversationId, agentId: null, title: selectedTitle, projectName: chat.projectName ?? "Conversa com Jarvito",
    request: chat.chat.pendingQuestion, requiresConversation: Boolean(chat.chat.pendingApproval || chat.chat.pendingAuthoring),
  }} drafts={questionDrafts} onAnswer={answer} onInteract={pause} onOpenConversation={() => { void invoke("companion_open_conversation", { conversationId: chat.conversationId }).catch(cause => setActionError(errorMessage(cause))); }} error={actionError || error} />;

  return <div role="region" aria-label="Conversa e controles do Jarvito" className="companion-chat flex h-full min-h-0 flex-col gap-2">
    <div className="flex items-center gap-1">
    <Select value={selected} onValueChange={value => { if (typeof value === "string") chooseConversation(value); }}>
      <SelectTrigger aria-label="Conversa do Jarvito" size="sm" disabled={busy} className="min-w-0 flex-1 cursor-pointer text-[11px]"><SelectValue>{selectedTitle}</SelectValue></SelectTrigger>
      <SelectContent className="max-h-64 max-w-[calc(100vw-36px)]">
        <SelectItem value="global" className="cursor-pointer text-[11px]"><span className="flex flex-col gap-0.5"><span>Conversar com Jarvito</span><span className="text-[9px] text-muted-foreground">Sem projeto · perguntas e ajuda do dia a dia</span></span></SelectItem>
        {conversations.map(item => <SelectItem key={item.id} value={item.id} className="cursor-pointer text-[11px]"><span className="flex min-w-0 flex-col gap-0.5"><span className="truncate">{item.title}</span><span className="truncate font-mono text-[9px] text-muted-foreground">{item.workspaceName} · {item.projectName}</span></span></SelectItem>)}
      </SelectContent>
    </Select>
    {selected === "global" && <Hint content={running ? "Pare a resposta antes de limpar a conversa." : clearBlocked ? "Aguarde as operações da conversa terminarem para limpar." : "Limpar histórico e começar uma nova conversa."}><Button type="button" aria-label="Limpar conversa" aria-busy={busy} variant="ghost" size="icon-sm" disabled={busy || loading || !chat?.global || clearBlocked} className="shrink-0 cursor-pointer text-muted-foreground" onClick={() => { void clear(); }}><Eraser className="size-3.5" /></Button></Hint>}
    </div>
    {chat?.global === false && <p className="px-1 text-[9px] text-muted-foreground">Projeto {chat.projectName} · conversa compartilhada com o Jarvis</p>}
    <ScrollArea ref={scroll} className="min-h-0 flex-1 pr-2" onScrollCapture={event => {
      const target = event.target;
      if (target instanceof HTMLElement) stickToBottom.current = target.scrollHeight - target.scrollTop - target.clientHeight < 40;
    }}>
      <div role="log" aria-label="Mensagens do Jarvito" aria-live="polite" className="space-y-3 pb-2">
        {loading && <div role="status" aria-label="Carregando conversa" className="space-y-3 py-2"><Skeleton className="ml-auto h-8 w-3/5" /><Skeleton className="h-16 w-4/5" /></div>}
        {!loading && chat && !chat.chat.turns.length && <div className="flex flex-col items-center gap-2 py-7 text-center"><MessageCircle className="size-4 text-onedark-cyan/70" /><p className="text-xs font-medium">Oi, eu sou o Jarvito.</p><p className="max-w-60 text-[11px] leading-5 text-muted-foreground">Posso ajudar por aqui e acompanhar suas conversas sem abrir a janela principal.</p></div>}
        {chat?.chat.history && chat.chat.history.start > 0 && <p className="text-center text-[9px] text-muted-foreground">Mostrando as mensagens recentes desta conversa.</p>}
        {chat?.chat.turns.map(turn => <section key={turn.id} aria-label="Troca de mensagens" className="space-y-2">
          <p className="ml-7 whitespace-pre-wrap break-words rounded-2xl rounded-br-md border border-border bg-secondary/55 px-3 py-2 text-[11px] leading-5">{turn.user}</p>
          {turn.auxiliaryMessages?.map(message => <p key={message.id} className="ml-7 whitespace-pre-wrap break-words rounded-2xl rounded-br-md border border-border bg-secondary/55 px-3 py-2 text-[11px] leading-5">{message.content}</p>)}
          {turn.steps.some(step => step.text) && <div className="companion-chat-answer mr-3 text-[11px] leading-5">{chat.global ? <p className="whitespace-pre-wrap break-words">{turn.steps.map(step => step.text).filter(Boolean).join("\n\n")}</p> : <LazyChatMarkdown content={turn.steps.map(step => step.text).filter(Boolean).join("\n\n")} />}</div>}
          {turn.error && <p role="alert" className="text-[11px] leading-5 text-onedark-red">{turn.error.message}</p>}
          {turn.status === "running" && <p role="status" className="flex items-center gap-2 text-[10px] text-onedark-cyan"><span aria-hidden="true" className="size-1.5 rounded-full bg-current" />{chat.global ? "Jarvito está pensando…" : turn.steps[turn.steps.length - 1]?.summary || "Jarvito está pensando…"}</p>}
        </section>)}
        {chat?.chat.queuedMessages?.map(message => <div key={message.id} className="ml-7 space-y-1 rounded-xl border border-border bg-secondary/40 px-3 py-2"><p className="whitespace-pre-wrap break-words text-[11px] leading-5">{message.content}</p><Badge variant="outline" className="font-mono text-[9px]">Aguardando envio</Badge></div>)}
        {chat?.proposal && <section aria-label="Continuar em um projeto" className="space-y-2 rounded-xl border border-onedark-cyan/30 bg-onedark-cyan/5 p-3">
          <p className="text-xs font-medium">{chat.proposal.conversationId ? "Continuar a conversa" : "Criar conversa"} em {chat.proposal.projectName}?</p>
          {chat.proposal.execution && <Badge variant="outline" className="max-w-full text-[10px]"><span className="truncate">{chat.proposal.execution.kind === "flow" ? "Fluxo" : "Agente"}: {chat.proposal.execution.name}</span></Badge>}
          <p className="text-[10px] leading-4 text-muted-foreground">{chat.proposal.reason}</p>
          <p className="whitespace-pre-wrap break-words text-[11px] leading-5">{chat.proposal.message}</p>
          <div className="flex gap-2"><Button size="sm" disabled={busy} className="h-7 cursor-pointer text-[10px]" onClick={() => { void confirm(true); }}><Check className="size-3" />Confirmar</Button><Button variant="ghost" size="sm" disabled={busy} className="h-7 cursor-pointer text-[10px]" onClick={() => { void confirm(false); }}><X className="size-3" />Agora não</Button></div>
        </section>}
        {(actionError || error) && <div className="space-y-1"><p role="alert" className="text-[11px] leading-5 text-onedark-yellow">{actionError || error}</p>{preservedRequest && preservedRequest !== draft && <p className="whitespace-pre-wrap break-words rounded-lg border border-border p-2 text-[11px] leading-5">{preservedRequest}</p>}{!chat && <Button size="sm" variant="ghost" className="h-7 cursor-pointer text-[10px]" onClick={() => { setLoading(true); setError(null); setAttempt(value => value + 1); }}>Tentar novamente</Button>}</div>}
      </div>
    </ScrollArea>
    <form className="relative shrink-0" onSubmit={event => { event.preventDefault(); void send(); }}>
      <Textarea aria-label="Mensagem para Jarvito" placeholder={running ? "Envie uma orientação…" : "Converse com Jarvito…"} value={draft} onChange={event => { drafts.current.set(draftKey, event.target.value); setDraft(event.target.value); }} className={`min-h-14 max-h-24 resize-none rounded-2xl border-border bg-secondary/40 text-[11px] leading-5 ${running ? "pr-20" : "pr-11"}`} onKeyDown={event => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void send(); } }} />
      <div className="absolute right-2 bottom-2 flex items-center gap-1">
        {running && <Hint content="Parar resposta"><Button type="button" variant="secondary" size="icon-sm" aria-label="Parar resposta" disabled={busy} className="cursor-pointer rounded-full" onClick={() => { void stop(); }}><Square className="size-3" /></Button></Hint>}
        <Button type="submit" size="icon-sm" aria-label={running ? "Enviar orientação agora" : "Enviar mensagem"} aria-busy={busy} disabled={busy || loading || modelInvalid || !draft.trim()} className="cursor-pointer rounded-full"><ArrowUp className="size-3.5" /></Button>
      </div>
    </form>
    <div className="-mx-1 flex shrink-0 items-center overflow-hidden">
      <ModelPicker modelGroups={models} selection={selectedModel} onSelect={selection => { void chooseModel(selection); }} disabled={busy || loading || running} invalid={modelInvalid} showProviderIdentity ariaLabel="Modelo do Jarvito" />
    </div>
  </div>;
}
