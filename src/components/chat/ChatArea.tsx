import { useEffect, useRef, useState, type ReactNode } from "react";
import { FolderGit2, MessageSquare } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { ConversationSkeleton } from "@/components/layout/LoadingSkeletons";
import type { ChatDraft } from "@/core/chat";
import { questionKey, type QuestionDraft } from "@/core/questions";
import type { ConversationDetails, LibrarySnapshot } from "@/core/library";
import type { ChatController } from "@/hooks/use-chat";
import { ChatComposer, type ProviderModelGroup } from "./ChatComposer";
import { Transcript, type LatestVisibility } from "./Transcript";
import { ToolApproval } from "./ToolApproval";
import { QuestionCard } from "./QuestionCard";
import { WorkerRequests } from "./WorkerRequests";
import type { AgentModelsController } from "@/hooks/use-agent-models";
import type { WorkflowController } from "@/hooks/use-workflow";

function ConversationView({ context, modelGroups, chat, workflow, agentModels, drafts, questionDrafts, onLatestVisibility }: { context: ConversationDetails; modelGroups: ProviderModelGroup[]; chat: ChatController; workflow?: WorkflowController; agentModels?: AgentModelsController; drafts: Map<string, ChatDraft>; questionDrafts: Map<string, QuestionDraft>; onLatestVisibility?: LatestVisibility }) {
  const footer = useRef<HTMLElement>(null);
  const previousQuestion = useRef<string | undefined>(undefined);
  const snapshot = chat.snapshot;
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
  return <>
    <Transcript snapshot={snapshot} chat={chat} onLatestVisibility={onLatestVisibility} />
    <footer ref={footer} aria-label="Área de composição" className="chat-footer mx-auto w-full max-w-4xl min-w-0 shrink-0 max-h-[65%] overflow-y-auto overscroll-none px-5 pb-4 pt-3">
      {snapshot.pendingApproval && <ToolApproval key={snapshot.pendingApproval.id} tool={snapshot.pendingApproval} projectPath={context.project.path} onAnswer={chat.approve} />}
      {snapshot.pendingQuestion && <QuestionCard key={questionKey(context.conversation.id, snapshot.pendingQuestion)} request={snapshot.pendingQuestion} drafts={questionDrafts} draftKey={questionKey(context.conversation.id, snapshot.pendingQuestion)} onAnswer={chat.answerQuestion} />}
      <WorkerRequests conversationId={context.conversation.id} projectPath={context.project.path} agents={workflow?.data?.agents ?? []} drafts={questionDrafts} />
      <ChatComposer agentModels={agentModels} compacting={chat.compacting} drafts={drafts} draftKey={context.conversation.id} queuedMessages={snapshot.queuedMessages} onRemoveQueued={chat.removeQueued} onResumeQueue={chat.resumeQueue} running={snapshot.activeTurnId !== null} onStop={chat.stop} onSendMessage={chat.send} modelGroups={modelGroups} initialOptions={snapshot.latestOptions ?? last?.options} />
      {modelGroups.length === 0 && <p className="mt-2 text-center text-xs text-muted-foreground">Conecte uma conta em Configurações para enviar mensagens.</p>}
    </footer>
  </>;
}

export function ChatArea({ modelGroups = [], library, chat, workflow, agentModels, leftToggle, rightToggle, onLatestVisibility }: { modelGroups?: ProviderModelGroup[]; library: LibrarySnapshot | null; chat: ChatController; workflow?: WorkflowController; agentModels?: AgentModelsController; leftToggle?: ReactNode; rightToggle?: ReactNode; onLatestVisibility?: LatestVisibility }) {
  const [drafts] = useState(() => new Map<string, ChatDraft>());
  const [questionDrafts] = useState(() => new Map<string, QuestionDraft>());
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
    {!library ? <ConversationSkeleton /> : id && project && workspace && conversation ? <ConversationView key={id} drafts={drafts} questionDrafts={questionDrafts} context={{ workspace, project, conversation }} modelGroups={modelGroups} chat={chat} workflow={workflow} agentModels={agentModels} onLatestVisibility={onLatestVisibility} /> : <Empty className="flex-1"><EmptyHeader><EmptyMedia variant="icon"><MessageSquare /></EmptyMedia><EmptyTitle>{project ? "Inicie uma conversa" : "Seu próximo projeto começa aqui"}</EmptyTitle><EmptyDescription>{project ? `Crie ou selecione uma conversa em ${project.name} pela barra lateral.` : "Selecione um projeto na barra lateral ou crie um workspace para organizar seu trabalho."}</EmptyDescription></EmptyHeader></Empty>}
  </main>;
}
