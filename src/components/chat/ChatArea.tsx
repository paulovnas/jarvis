import { JARVITO_CHAT_ID } from "@/core/companion";
import { useEffect, useRef, useState } from "react";
import { Globe, MessageSquare, Send } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { ConversationSkeleton } from "@/components/layout/LoadingSkeletons";
import type { ChatDraft, TurnOptions } from "@/core/chat";
import type { ExecutionChoice } from "@/core/executors";
import type { ModelBinding } from "@/core/provider-references";
import type { ModelCatalogRefresh } from "@/core/provider-accounts";
import { questionKey, type QuestionDraft } from "@/core/questions";
import type { ConversationDetails, LibrarySnapshot } from "@/core/library";
import type { ChatController } from "@/hooks/use-chat";
import { ChatComposer, type ProviderModelGroup } from "./ChatComposer";
import { Transcript, type LatestVisibility } from "./Transcript";
import { ToolApproval } from "./ToolApproval";
import { QuestionCard } from "./QuestionCard";
import { WorkerRequests } from "./WorkerRequests";
import type { AgentModelsController } from "@/hooks/use-agent-models";
import { useChatAgentModels } from "@/hooks/use-chat-agent-models";
import type { WorkflowController } from "@/hooks/use-workflow";
import type { ProjectFilesController } from "@/hooks/use-project-files";
import { FileWorkspace } from "@/components/files/FileWorkspace";
import { httpAnalysisPrompt, useHttpClient, type HttpTab } from "@/hooks/use-http-client";
import { useBrowser } from "@/hooks/use-browser";
import { JarvisLogo } from "@/components/JarvisLogo";
import { AuthoringApprovalDrawer } from "./AuthoringApprovalDrawer";
import { WorkflowRecoveryAlert } from "./WorkflowRecoveryAlert";
import { Hint } from "@/components/ui/hint";
import { ActiveExecutionStatus } from "./ActiveExecutionStatus";
import { ComputerUseSession } from "./ComputerUseSession";
import { useVideoReady } from "@/hooks/use-video-ready";
import { useImpeccableLive } from "@/hooks/use-impeccable-live";
import type { DesignerSession } from "@/components/layout/DesignerPanel";

function ConversationView({ context, modelGroups, modelBindings, modelsReady, onRefreshModels, refreshingModels, chat, workflow, agentModels, drafts, questionDrafts, httpDrafts, onLatestVisibility, files, onDesignerChange }: { context: ConversationDetails; modelGroups: ProviderModelGroup[]; modelBindings?: ModelBinding[]; modelsReady?: boolean; onRefreshModels?: () => Promise<ModelCatalogRefresh | null>; refreshingModels?: boolean; chat: ChatController; workflow?: WorkflowController; agentModels?: AgentModelsController; drafts: Map<string, ChatDraft>; questionDrafts: Map<string, QuestionDraft>; httpDrafts: Map<string, HttpTab[]>; onLatestVisibility?: LatestVisibility; files?: ProjectFilesController; onDesignerChange?: (session: DesignerSession | null) => void }) {
  const footer = useRef<HTMLElement>(null);
  const previousQuestion = useRef<string | undefined>(undefined);
  const snapshot = chat.snapshot;
  const chatModels = useChatAgentModels(context.conversation.id);
  const [retryChoice, setRetryChoice] = useState<ExecutionChoice | null>(null);
  const projectId = context.project?.id ?? "";
  const directory = context.project?.path ?? context.directory;
  const global = context.conversation.projectId === null;
  useVideoReady(projectId, context.conversation.id, files, snapshot);
  const http = useHttpClient(context.conversation.id, projectId, !!snapshot && !global, httpDrafts);
  const [draftInsertion, setDraftInsertion] = useState<{ id: string; text: string }>();
  const browser = useBrowser(context.conversation.id, () => { files?.select(null); http.select(null); }, !!snapshot);
  const activeBrowser = browser.snapshot.tabs.find(tab => tab.id === browser.snapshot.activeId);
  const liveOptions: TurnOptions | undefined = retryChoice ? { ...retryChoice, mode: "build", workflow: "designer", approvalMode: "yolo", manualValidation: false } : undefined;
  const attention = [snapshot?.pendingApproval?.tool.id, snapshot?.pendingQuestion?.toolId, snapshot?.pendingAuthoring?.toolId, ...(workflow?.data?.agents ?? []).flatMap(agent => [agent.pendingApproval?.tool.id, agent.pendingQuestion?.toolId, agent.pendingAuthoring?.toolId])].filter(Boolean).join("|");
  const designLive = useImpeccableLive(context.conversation.id, !!snapshot && !global, { options: liveOptions, url: activeBrowser?.url, tabId: activeBrowser?.id, canResume: !!liveOptions && !chat.compacting && !attention && !global, blockedReason: attention ? "Responda à solicitação no chat para continuar a alteração Live." : undefined });
  const currentDesigner = useRef({ designLive, liveOptions, browser, files, http });
  useEffect(() => { currentDesigner.current = { designLive, liveOptions, browser, files, http }; });
  const { status: liveStatus, active: liveActive, loaded: liveLoaded, busy: liveBusy } = designLive;
  const canResumeLive = !!liveOptions && !chat.compacting && !attention;
  useEffect(() => {
    if (global) { onDesignerChange?.(null); return; }
    onDesignerChange?.({
      conversationId: context.conversation.id,
      live: {
        status: liveStatus, active: liveActive, loaded: liveLoaded, busy: liveBusy,
        toggle: (...args) => currentDesigner.current.designLive.toggle(...args),
        resume: (...args) => currentDesigner.current.designLive.resume(...args),
        editTask: (...args) => currentDesigner.current.designLive.editTask(...args),
        removeTask: (...args) => currentDesigner.current.designLive.removeTask(...args),
        resumeTask: (...args) => currentDesigner.current.designLive.resumeTask(...args),
        cancelAction: (...args) => currentDesigner.current.designLive.cancelAction(...args),
      },
      canResume: canResumeLive, attention: !!attention,
      onResume: async () => {
        const current = currentDesigner.current;
        const selected = current.browser.snapshot.tabs.find(tab => tab.id === current.browser.snapshot.activeId);
        const url = selected?.id === current.designLive.status.tabId ? selected?.url : current.designLive.status.url ?? undefined;
        if (current.liveOptions) await current.designLive.resume(current.liveOptions, url);
      },
      onShowChat: () => { const current = currentDesigner.current; current.browser.select(null); current.files?.select(null); current.http.select(null); },
    });
  }, [onDesignerChange, context.conversation.id, liveStatus, liveActive, liveLoaded, liveBusy, canResumeLive, attention, global]);
  useEffect(() => () => { onDesignerChange?.(null); }, [onDesignerChange, context.conversation.id]);
  const liveBrowser = !!activeBrowser && (activeBrowser.id === designLive.status.tabId || (!designLive.status.tabId && !!designLive.status.url && activeBrowser.url === designLive.status.url));
  const previousAttention = useRef("");
  useEffect(() => {
    if (attention !== previousAttention.current) {
      previousAttention.current = attention;
      if (attention && (files?.tabs.activePath || browser.snapshot.activeId || http.activeId) && !liveBrowser) { browser.select(null); files?.select(null); http.select(null); }
    }
  }, [attention, browser, files, http, liveBrowser]);
  useEffect(() => {
    const question = snapshot?.pendingQuestion;
    if (previousQuestion.current && !question) {
      questionDrafts.delete(previousQuestion.current);
      footer.current?.querySelector<HTMLElement>('[role="textbox"][contenteditable="true"]')?.focus();
    }
    previousQuestion.current = question ? questionKey(context.conversation.id, question) : undefined;
  }, [snapshot?.pendingQuestion, context.conversation.id, questionDrafts]);
  if (chat.error && !snapshot) return <Empty><EmptyHeader><EmptyTitle>Não foi possível abrir a conversa</EmptyTitle><EmptyDescription role="alert">{chat.error}</EmptyDescription></EmptyHeader><Button className="cursor-pointer" variant="outline" onClick={chat.retry}>Tentar novamente</Button></Empty>;
  if (!snapshot) return <ConversationSkeleton />;
  const composerOptions = [...snapshot.turns]
    .reverse()
    .find(turn => turn.options.workflow !== "publication")?.options;
  const outsideChat = !!files?.tabs.activePath || !!browser.snapshot.activeId || !!http.activeId;
  const isNewConversation = (snapshot.history?.total ?? snapshot.turns.length) === 0
    && snapshot.activeTurnId === null
    && !chat.pendingTurn
    && !snapshot.pendingApproval
    && !snapshot.pendingQuestion
    && !snapshot.pendingAuthoring
    && !snapshot.queuedMessages?.length;
  const embeddedBrowser = browser.loaded && browser.snapshot.backend === "embedded";
  const liveBanner = liveBrowser && attention && <Alert role="status" aria-label="Solicitação pendente no Live" className="mx-3 mb-2 flex w-auto items-center justify-between gap-3 py-2"><AlertDescription className="text-xs">Uma solicitação precisa da sua resposta para continuar.</AlertDescription><Button size="sm" variant="outline" className="h-7 shrink-0 cursor-pointer text-xs" onClick={() => browser.select(null)}>Responder no chat</Button></Alert>;
  const openHttp = () => { files?.select(null); browser.select(null); void http.open(); };
  const browserLauncher = embeddedBrowser && <Hint content="Abrir navegador"><Button type="button" variant="ghost" size="icon" aria-label="Abrir navegador" disabled={browser.busy} onClick={() => void browser.open()} className="size-7 cursor-pointer text-muted-foreground hover:text-onedark-cyan"><Globe className="size-3.5" /></Button></Hint>;
  const httpLauncher = !global && <Hint content="Nova requisição HTTP"><Button type="button" variant="ghost" size="icon" aria-label="Nova requisição HTTP" disabled={!!http.busy} onClick={openHttp} className="size-7 cursor-pointer text-muted-foreground hover:text-onedark-green"><Send className="size-3.5" /></Button></Hint>;
  const activeTurn = snapshot.activeTurnId
    ? chat.pendingTurn?.id === snapshot.activeTurnId
      ? chat.pendingTurn
      : snapshot.turns.find(turn => turn.id === snapshot.activeTurnId)
    : chat.pendingTurn?.status === "running" && !snapshot.turns.some(turn => turn.id === chat.pendingTurn?.id)
      ? chat.pendingTurn
      : undefined;
  const composer = <footer ref={footer} aria-label="Área de composição" className="chat-footer mx-auto max-h-[65dvh] w-full max-w-4xl min-w-0 shrink-0 overflow-y-auto overscroll-none px-5 pt-3">
    {chat.error && <Alert className="mb-3"><AlertTitle>Atualizações da conversa indisponíveis</AlertTitle><AlertDescription className="flex flex-wrap items-center justify-between gap-2"><p>{chat.error}</p><Button variant="outline" size="sm" className="cursor-pointer" onClick={chat.retry}>Tentar novamente</Button></AlertDescription></Alert>}
    {activeTurn && <ActiveExecutionStatus turn={activeTurn} waitingForUser={!!attention} />}
    {workflow?.data?.recovery && <WorkflowRecoveryAlert recovery={workflow.data.recovery} onResume={chat.resumeWorkflow} />}
    {snapshot.pendingApproval && <ToolApproval key={snapshot.pendingApproval.tool.id} request={snapshot.pendingApproval} projectPath={directory ?? ""} conversationId={context.conversation.id} onAnswer={chat.approve} />}
    {snapshot.pendingQuestion && <QuestionCard key={questionKey(context.conversation.id, snapshot.pendingQuestion)} request={snapshot.pendingQuestion} drafts={questionDrafts} draftKey={questionKey(context.conversation.id, snapshot.pendingQuestion)} onAnswer={chat.answerQuestion} onInteract={chat.pauseQuestion} />}
    {snapshot.pendingAuthoring && <AuthoringApprovalDrawer key={`${snapshot.pendingAuthoring.turnId}:${snapshot.pendingAuthoring.toolId}`} request={snapshot.pendingAuthoring} onAnswer={(approved, note, values) => chat.answerAuthoring(snapshot.pendingAuthoring!, approved, note, values)} />}
    <WorkerRequests conversationId={context.conversation.id} projectPath={directory ?? ""} agents={workflow?.data?.agents ?? []} drafts={questionDrafts} />
    <ChatComposer computerControl={<ComputerUseSession key={context.conversation.id} conversationId={context.conversation.id} onStop={chat.stop} />} projectless={global} companion={global && context.conversation.id === JARVITO_CHAT_ID} designLive={global ? undefined : designLive} liveUrl={browser.snapshot.tabs.find(tab => tab.id === browser.snapshot.activeId)?.url} draftInsertion={draftInsertion} onOpenBrowser={embeddedBrowser ? () => void browser.open() : undefined} browserBusy={browser.busy} onOpenHttp={global ? undefined : openHttp} httpBusy={!!http.busy} agentModels={agentModels} chatModels={chatModels} compacting={chat.compacting} drafts={drafts} draftKey={context.conversation.id} historySnapshot={snapshot} queuedMessages={snapshot.queuedMessages} onRemoveQueued={chat.removeQueued} onDeleteQueued={chat.deleteQueued} onSendQueuedNow={chat.sendQueuedNow} onReorderQueued={chat.reorderQueued} onResumeQueue={chat.resumeQueue} running={snapshot.activeTurnId !== null} onStop={chat.stop} onSendMessage={chat.send} onExecutionChoiceChange={setRetryChoice} modelGroups={modelGroups} modelBindings={modelBindings} modelsReady={modelsReady} onRefreshModels={onRefreshModels} refreshingModels={refreshingModels} initialOptions={composerOptions} workflowSnapshot={workflow?.data} />
    {modelGroups.length === 0 && <p className="mt-2 text-center text-xs text-muted-foreground">Conecte uma conta em Configurações para enviar mensagens.</p>}
  </footer>;
  return <FileWorkspace files={files} browser={browser} http={http} liveTabId={designLive.active ? designLive.status.tabId : null} browserBanner={liveBanner} onAnalyzeHttp={run => { http.select(null); browser.select(null); files?.select(null); setDraftInsertion({ id: crypto.randomUUID(), text: httpAnalysisPrompt(run) }); }} terminalLauncher={outsideChat ? <>{browserLauncher}{httpLauncher}</> : undefined}>
    <section aria-label={isNewConversation ? "Nova conversa" : undefined} data-empty={isNewConversation} className="new-conversation-stage relative isolate grid min-h-0 flex-1 overflow-hidden">
      <div className="conversation-transcript-slot relative z-10 flex min-h-0 overflow-hidden">
        {!isNewConversation && <Transcript snapshot={snapshot} chat={chat} retryChoice={retryChoice} projectId={context.project?.id} onLatestVisibility={onLatestVisibility} />}
      </div>
      <div className="new-conversation-brand-slot relative z-10 min-h-0 overflow-hidden" aria-hidden={!isNewConversation}>
        <div className="new-conversation-brand flex flex-col items-center gap-1 pb-5 text-center">
          <JarvisLogo variant="horizontal" alt="Jarvis" className="h-auto w-64 max-w-full" />
          <h2 className="text-lg font-medium tracking-tight text-foreground">O que vamos construir?</h2>
        </div>
      </div>
      <div className="conversation-composer-slot relative z-20 mx-auto w-full min-w-0">
        {composer}
      </div>
      <div aria-hidden="true" className="new-conversation-spacer min-h-0" />
    </section>
  </FileWorkspace>;
}

export function ChatArea({ modelGroups = [], modelBindings, modelsReady, onRefreshModels, refreshingModels, library, chat, workflow, agentModels, onLatestVisibility, files, drafts: sharedDrafts, questionDrafts: sharedQuestionDrafts, onDesignerChange }: { modelGroups?: ProviderModelGroup[]; modelBindings?: ModelBinding[]; modelsReady?: boolean; onRefreshModels?: () => Promise<ModelCatalogRefresh | null>; refreshingModels?: boolean; library: LibrarySnapshot | null; chat: ChatController; workflow?: WorkflowController; agentModels?: AgentModelsController; onLatestVisibility?: LatestVisibility; files?: ProjectFilesController; drafts?: Map<string, ChatDraft>; questionDrafts?: Map<string, QuestionDraft>; onDesignerChange?: (session: DesignerSession | null) => void }) {
  const [httpDrafts] = useState(() => new Map<string, HttpTab[]>());
  const [localDrafts] = useState(() => new Map<string, ChatDraft>());
  const [localQuestionDrafts] = useState(() => new Map<string, QuestionDraft>());
  const drafts = sharedDrafts ?? localDrafts;
  const questionDrafts = sharedQuestionDrafts ?? localQuestionDrafts;
  const id = library?.selection.conversationId;
  const project = library?.projects.find(item => item.id === library.selection.projectId);
  const workspace = library?.workspaces.find(item => item.id === project?.workspaceId);
  const conversation = library?.conversations.find(item => item.id === id);
  const global = conversation?.projectId === null;
  return <main aria-label="Conversa" aria-description={conversation?.title} className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-background">
    {!library ? <ConversationSkeleton /> : id && conversation && (global || project && workspace) ? <ConversationView key={`${id}:${conversation.projectId ?? "global"}:${global ? library.globalDirectory : ""}`} httpDrafts={httpDrafts} drafts={drafts} questionDrafts={questionDrafts} context={{ workspace: workspace ?? null, project: project ?? null, conversation, directory: library.globalDirectory }} modelGroups={modelGroups} modelBindings={modelBindings} modelsReady={modelsReady} onRefreshModels={onRefreshModels} refreshingModels={refreshingModels} chat={chat} workflow={workflow} agentModels={agentModels} onLatestVisibility={onLatestVisibility} files={files} onDesignerChange={onDesignerChange} /> : <Empty className="flex-1"><EmptyHeader><EmptyMedia variant="icon"><MessageSquare /></EmptyMedia><EmptyTitle>{project ? "Inicie uma conversa" : "Seu próximo projeto começa aqui"}</EmptyTitle><EmptyDescription>{project ? `Crie ou selecione uma conversa em ${project.name} pela barra lateral.` : "Selecione um projeto na barra lateral ou crie um espaço para organizar seu trabalho."}</EmptyDescription></EmptyHeader></Empty>}
  </main>;
}
