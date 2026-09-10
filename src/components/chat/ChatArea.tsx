import { useEffect, useRef, useState, type ReactNode } from "react";
import { FolderGit2, Globe, MessageSquare } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { ConversationSkeleton } from "@/components/layout/LoadingSkeletons";
import type { ChatDraft } from "@/core/chat";
import type { ModelBinding } from "@/core/provider-references";
import { questionKey, type QuestionDraft } from "@/core/questions";
import type { ConversationDetails, LibrarySnapshot } from "@/core/library";
import type { ChatController } from "@/hooks/use-chat";
import { ChatComposer, type ProviderModelGroup } from "./ChatComposer";
import { TerminalWorkspace } from "./TerminalWorkspace";
import { Transcript, type LatestVisibility } from "./Transcript";
import { ToolApproval } from "./ToolApproval";
import { QuestionCard } from "./QuestionCard";
import { WorkerRequests } from "./WorkerRequests";
import type { AgentModelsController } from "@/hooks/use-agent-models";
import type { WorkflowController } from "@/hooks/use-workflow";
import type { ProjectFilesController } from "@/hooks/use-project-files";
import { FileWorkspace } from "@/components/files/FileWorkspace";
import { useBrowser } from "@/hooks/use-browser";
import { JarvisLogo } from "@/components/JarvisLogo";
import { AuthoringApprovalDrawer } from "./AuthoringApprovalDrawer";

function ConversationView({ context, modelGroups, modelBindings, modelsReady, chat, workflow, agentModels, drafts, questionDrafts, onLatestVisibility, files }: { context: ConversationDetails; modelGroups: ProviderModelGroup[]; modelBindings?: ModelBinding[]; modelsReady?: boolean; chat: ChatController; workflow?: WorkflowController; agentModels?: AgentModelsController; drafts: Map<string, ChatDraft>; questionDrafts: Map<string, QuestionDraft>; onLatestVisibility?: LatestVisibility; files?: ProjectFilesController }) {
  const footer = useRef<HTMLElement>(null);
  const previousQuestion = useRef<string | undefined>(undefined);
  const snapshot = chat.snapshot;
  const browser = useBrowser(context.conversation.id, () => files?.select(null), !!snapshot);
  const attention = [snapshot?.pendingApproval?.id, snapshot?.pendingQuestion?.toolId, snapshot?.pendingAuthoring?.toolId, ...(workflow?.data?.agents ?? []).flatMap(agent => [agent.pendingApproval?.id, agent.pendingQuestion?.toolId, agent.pendingAuthoring?.toolId])].filter(Boolean).join("|");
  const previousAttention = useRef("");
  useEffect(() => {
    if (attention !== previousAttention.current) {
      previousAttention.current = attention;
      if (attention && browser.snapshot.activeId) { browser.select(null); files?.select(null); }
    }
  }, [attention, browser, files]);
  useEffect(() => {
    const question = snapshot?.pendingQuestion;
    if (previousQuestion.current && !question) {
      questionDrafts.delete(previousQuestion.current);
      footer.current?.querySelector<HTMLElement>('[role="textbox"][contenteditable="true"]')?.focus();
    }
    previousQuestion.current = question ? questionKey(context.conversation.id, question) : undefined;
  }, [snapshot?.pendingQuestion, context.conversation.id, questionDrafts]);
  if (chat.error) return <Empty><EmptyHeader><EmptyTitle>Não foi possível abrir a conversa</EmptyTitle><EmptyDescription role="alert">{chat.error}</EmptyDescription></EmptyHeader><Button className="cursor-pointer" variant="outline" onClick={chat.retry}>Tentar novamente</Button></Empty>;
  if (!snapshot) return <ConversationSkeleton />;
  const last = snapshot.turns[snapshot.turns.length - 1];
  const outsideChat = !!files?.tabs.activePath || !!browser.snapshot.activeId;
  const isNewConversation = (snapshot.history?.total ?? snapshot.turns.length) === 0
    && snapshot.activeTurnId === null
    && !snapshot.pendingApproval
    && !snapshot.pendingQuestion
    && !snapshot.pendingAuthoring
    && !snapshot.queuedMessages?.length;
  const browserLauncher = <Button type="button" variant="ghost" size="icon" aria-label="Abrir navegador" title="Abrir navegador" disabled={browser.busy} onClick={() => void browser.open()} className="size-7 cursor-pointer text-muted-foreground hover:text-onedark-cyan"><Globe className="size-3.5" /></Button>;
  const composer = (terminalLauncher: ReactNode) => <footer ref={footer} aria-label="Área de composição" className="chat-footer mx-auto max-h-[65dvh] w-full max-w-4xl min-w-0 shrink-0 overflow-y-auto overscroll-none px-5 pb-4 pt-3">
    {snapshot.pendingApproval && <ToolApproval key={snapshot.pendingApproval.id} tool={snapshot.pendingApproval} projectPath={context.project.path} onAnswer={chat.approve} />}
    {snapshot.pendingQuestion && <QuestionCard key={questionKey(context.conversation.id, snapshot.pendingQuestion)} request={snapshot.pendingQuestion} drafts={questionDrafts} draftKey={questionKey(context.conversation.id, snapshot.pendingQuestion)} onAnswer={chat.answerQuestion} />}
    {snapshot.pendingAuthoring && <AuthoringApprovalDrawer key={snapshot.pendingAuthoring.toolId} request={snapshot.pendingAuthoring} onAnswer={(approved, note) => chat.answerAuthoring(snapshot.pendingAuthoring!, approved, note)} />}
    <WorkerRequests conversationId={context.conversation.id} projectPath={context.project.path} agents={workflow?.data?.agents ?? []} drafts={questionDrafts} />
    <ChatComposer terminalLauncher={outsideChat ? undefined : <>{terminalLauncher}{browserLauncher}</>} agentModels={agentModels} compacting={chat.compacting} drafts={drafts} draftKey={context.conversation.id} queuedMessages={snapshot.queuedMessages} onRemoveQueued={chat.removeQueued} onDeleteQueued={chat.deleteQueued} onSendQueuedNow={chat.sendQueuedNow} onReorderQueued={chat.reorderQueued} onResumeQueue={chat.resumeQueue} running={snapshot.activeTurnId !== null} onStop={chat.stop} onSendMessage={chat.send} modelGroups={modelGroups} modelBindings={modelBindings} modelsReady={modelsReady} initialOptions={snapshot.latestOptions ?? last?.options} workflowSnapshot={workflow?.data} />
    {modelGroups.length === 0 && <p className="mt-2 text-center text-xs text-muted-foreground">Conecte uma conta em Configurações para enviar mensagens.</p>}
  </footer>;
  return <TerminalWorkspace conversationId={context.conversation.id}>{terminalLauncher => <FileWorkspace files={files} browser={browser} terminalLauncher={outsideChat ? <>{terminalLauncher}{browserLauncher}</> : undefined}>
    <section aria-label={isNewConversation ? "Nova conversa" : undefined} data-empty={isNewConversation} className="new-conversation-stage relative isolate grid min-h-0 flex-1 overflow-hidden">
      <div aria-hidden="true" className="new-conversation-glow pointer-events-none absolute left-1/2 top-1/2 h-64 w-[min(90%,56rem)] -translate-x-1/2 -translate-y-1/2" />
      <div className="conversation-transcript-slot relative z-10 flex min-h-0 overflow-hidden">
        {!isNewConversation && <Transcript snapshot={snapshot} chat={chat} onLatestVisibility={onLatestVisibility} />}
      </div>
      <div className="new-conversation-brand-slot relative z-10 min-h-0 overflow-hidden" aria-hidden={!isNewConversation}>
        <div className="new-conversation-brand flex flex-col items-center gap-1 pb-5 text-center">
          <JarvisLogo variant="vertical" alt="Jarvis" className="size-32" />
          <h2 className="text-lg font-medium tracking-tight text-foreground">O que vamos construir?</h2>
        </div>
      </div>
      <div className="conversation-composer-slot relative z-20 mx-auto w-full min-w-0">
        {composer(terminalLauncher)}
      </div>
      <div aria-hidden="true" className="new-conversation-spacer min-h-0" />
    </section>
  </FileWorkspace>}</TerminalWorkspace>;
}

export function ChatArea({ modelGroups = [], modelBindings, modelsReady, library, chat, workflow, agentModels, leftToggle, rightToggle, onLatestVisibility, files, drafts: sharedDrafts, questionDrafts: sharedQuestionDrafts }: { modelGroups?: ProviderModelGroup[]; modelBindings?: ModelBinding[]; modelsReady?: boolean; library: LibrarySnapshot | null; chat: ChatController; workflow?: WorkflowController; agentModels?: AgentModelsController; leftToggle?: ReactNode; rightToggle?: ReactNode; onLatestVisibility?: LatestVisibility; files?: ProjectFilesController; drafts?: Map<string, ChatDraft>; questionDrafts?: Map<string, QuestionDraft> }) {
  const [localDrafts] = useState(() => new Map<string, ChatDraft>());
  const [localQuestionDrafts] = useState(() => new Map<string, QuestionDraft>());
  const drafts = sharedDrafts ?? localDrafts;
  const questionDrafts = sharedQuestionDrafts ?? localQuestionDrafts;
  const id = library?.selection.conversationId;
  const project = library?.projects.find(item => item.id === library.selection.projectId);
  const workspace = library?.workspaces.find(item => item.id === project?.workspaceId);
  const conversation = library?.conversations.find(item => item.id === id);
  return <main aria-label="Conversa" className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-background">
    <header className="flex h-[72px] shrink-0 items-center gap-3 border-b border-border px-3">
      {leftToggle}
      <div className="flex size-8 shrink-0 items-center justify-center rounded-md border border-border bg-card text-primary shadow-[inset_0_1px_0_#ffffff0d]"><FolderGit2 aria-hidden="true" className="size-4" /></div>
      <div className="min-w-0 flex-1">
        <p className="mb-1 truncate font-mono text-[10px] text-muted-foreground" title={project?.path}>{workspace?.name ?? "Jarvis"}{project && <> <span className="px-1 text-muted-foreground/50">/</span> {project.name}</>}</p>
        {conversation && <h1 className="truncate text-sm font-medium">{conversation.title}</h1>}
      </div>
      {rightToggle}
    </header>
    {!library ? <ConversationSkeleton /> : id && project && workspace && conversation ? <ConversationView key={id} drafts={drafts} questionDrafts={questionDrafts} context={{ workspace, project, conversation }} modelGroups={modelGroups} modelBindings={modelBindings} modelsReady={modelsReady} chat={chat} workflow={workflow} agentModels={agentModels} onLatestVisibility={onLatestVisibility} files={files} /> : <Empty className="flex-1"><EmptyHeader><EmptyMedia variant="icon"><MessageSquare /></EmptyMedia><EmptyTitle>{project ? "Inicie uma conversa" : "Seu próximo projeto começa aqui"}</EmptyTitle><EmptyDescription>{project ? `Crie ou selecione uma conversa em ${project.name} pela barra lateral.` : "Selecione um projeto na barra lateral ou crie um workspace para organizar seu trabalho."}</EmptyDescription></EmptyHeader></Empty>}
  </main>;
}
