import { Fragment, useEffect, useRef, useState } from "react";
import { MessageSquare } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ConversationSkeleton } from "@/components/layout/LoadingSkeletons";
import type { ChatDraft } from "@/core/chat";
import { questionKey, type QuestionDraft } from "@/core/questions";
import type { ConversationDetails, LibrarySnapshot } from "@/core/library";
import type { ChatController } from "@/hooks/use-chat";
import { ChatComposer, type ProviderModelGroup } from "./ChatComposer";
import { AssistantMessageTurn } from "./AssistantMessageTurn";
import { UserMessageBubble } from "./UserMessageBubble";
import { ToolApproval } from "./ToolApproval";
import { QuestionCard } from "./QuestionCard";

function ConversationView({ context, modelGroups, chat, drafts, questionDrafts }: { context: ConversationDetails; modelGroups: ProviderModelGroup[]; chat: ChatController; drafts: Map<string, ChatDraft>; questionDrafts: Map<string, QuestionDraft> }) {
  const bottom = useRef<HTMLDivElement>(null);
  const footer = useRef<HTMLElement>(null);
  const previousQuestion = useRef<string | undefined>(undefined);
  const follow = useRef(true);
  const snapshot = chat.snapshot;
  useEffect(() => {
    const question = snapshot?.pendingQuestion;
    if (previousQuestion.current && !question) {
      questionDrafts.delete(previousQuestion.current);
      footer.current?.querySelector<HTMLElement>('[role="textbox"][contenteditable="true"]')?.focus();
    }
    previousQuestion.current = question ? questionKey(context.conversation.id, question) : undefined;
  }, [snapshot?.pendingQuestion, context.conversation.id, questionDrafts]);
  useEffect(() => { if (follow.current) bottom.current?.scrollIntoView?.({ block: "end" }); }, [snapshot?.revision]);
  if (chat.error) return <Empty><EmptyHeader><EmptyTitle>Não foi possível abrir a conversa</EmptyTitle><EmptyDescription role="alert">{chat.error}</EmptyDescription></EmptyHeader><Button className="cursor-pointer" variant="outline" onClick={chat.retry}>Tentar novamente</Button></Empty>;
  if (!snapshot) return <ConversationSkeleton />;
  const last = snapshot.turns[snapshot.turns.length - 1];
  return <>
    <header className="border-b border-border px-5 py-4">
      <p className="mb-1 truncate text-xs text-muted-foreground">{context.workspace.name} / {context.project.name}</p>
      <h1 className="truncate text-base font-semibold">{context.conversation.title}</h1>
      <p className="mt-1 truncate text-xs text-muted-foreground" title={context.project.path}>{context.project.path}</p>
    </header>
    <ScrollArea className="min-h-0 flex-1" onScrollCapture={event => {
      const target = event.target;
      if (target instanceof HTMLElement) follow.current = target.scrollHeight - target.scrollTop - target.clientHeight < 80;
    }}>
      {snapshot.turns.length === 0 ? <Empty className="py-16"><EmptyHeader><EmptyMedia variant="icon"><MessageSquare /></EmptyMedia><EmptyTitle>Conversa criada</EmptyTitle><EmptyDescription>Envie uma instrução para começar a trabalhar neste projeto.</EmptyDescription></EmptyHeader></Empty> : <div className="mx-auto w-full max-w-4xl min-w-0 px-5" aria-label="Histórico de mensagens">
        {snapshot.turns.map(turn => {
          const timestamp = new Date(turn.createdAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
          return <Fragment key={turn.id}>
            <UserMessageBubble message={{ id: turn.id, role: "user", content: turn.user, parts: turn.parts, timestamp }} />
            <AssistantMessageTurn message={{
              id: turn.id, role: "assistant", content: turn.steps[turn.steps.length - 1]?.text ?? "", timestamp,
              model: `${turn.options.account} / ${turn.options.model}`,
              streaming: turn.status === "running",
              work: turn.status === "running" || turn.steps.some((step, index) => step.summary || step.tools.length || (step.text && index < turn.steps.length - 1)) ? {
                durationSeconds: Math.round(turn.durationMs / 1000),
                steps: turn.steps.map((step, index) => ({ thinking: step.summary, tools: step.tools, commentary: index < turn.steps.length - 1 ? step.text : "" })),
              } : undefined,
              error: turn.error ? { title: turn.status === "cancelled" || turn.status === "interrupted" ? "Execução interrompida" : "Falha na execução", message: turn.error.message } : undefined,
            }} />
          </Fragment>;
        })}
      </div>}
      <div ref={bottom} />
    </ScrollArea>
    <footer ref={footer} className="max-h-[65%] overflow-auto px-5 pb-4 pt-2">
      {snapshot.pendingApproval && <ToolApproval key={snapshot.pendingApproval.id} tool={snapshot.pendingApproval} projectPath={context.project.path} onAnswer={chat.approve} />}
      {snapshot.pendingQuestion && <QuestionCard key={questionKey(context.conversation.id, snapshot.pendingQuestion)} request={snapshot.pendingQuestion} drafts={questionDrafts} draftKey={questionKey(context.conversation.id, snapshot.pendingQuestion)} onAnswer={chat.answerQuestion} />}
      <ChatComposer compacting={chat.compacting} drafts={drafts} draftKey={context.conversation.id} queuedMessages={snapshot.queuedMessages} onRemoveQueued={chat.removeQueued} onResumeQueue={chat.resumeQueue} running={snapshot.activeTurnId !== null} onStop={chat.stop} onSendMessage={chat.send} modelGroups={modelGroups} initialOptions={last?.options} />
      {modelGroups.length === 0 && <p className="mt-2 text-center text-xs text-muted-foreground">Conecte uma conta em Configurações para enviar mensagens.</p>}
    </footer>
  </>;
}

export function ChatArea({ modelGroups = [], library, chat }: { modelGroups?: ProviderModelGroup[]; library: LibrarySnapshot | null; chat: ChatController }) {
  const [drafts] = useState(() => new Map<string, ChatDraft>());
  const [questionDrafts] = useState(() => new Map<string, QuestionDraft>());
  const id = library?.selection.conversationId;
  const project = library?.projects.find(item => item.id === library.selection.projectId);
  const workspace = library?.workspaces.find(item => item.id === project?.workspaceId);
  const conversation = library?.conversations.find(item => item.id === id);
  return <main aria-label="Conversa" className="flex h-full min-h-0 min-w-0 flex-col bg-background">
    {!library ? <ConversationSkeleton /> : id && project && workspace && conversation ? <ConversationView key={id} drafts={drafts} questionDrafts={questionDrafts} context={{ workspace, project, conversation }} modelGroups={modelGroups} chat={chat} /> : <Empty className="flex-1"><EmptyHeader><EmptyMedia variant="icon"><MessageSquare /></EmptyMedia><EmptyTitle>{project ? "Inicie uma conversa" : "Seu próximo projeto começa aqui"}</EmptyTitle><EmptyDescription>{project ? `Crie ou selecione uma conversa em ${project.name} pela barra lateral.` : "Selecione um projeto na barra lateral ou crie um workspace para organizar seu trabalho."}</EmptyDescription></EmptyHeader></Empty>}
  </main>;
}
