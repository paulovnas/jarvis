import { useCallback, useEffect, useId, useRef, useState } from "react";
import { ArrowUpRight, Check, ChevronLeft, ChevronRight, MessageCircle, Timer } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { PendingQuestion, QuestionDraft, QuestionResponse } from "@/core/questions";

export interface CompanionQuestionContext {
  conversationId: string;
  agentId: string | null;
  title: string;
  projectName: string;
  request?: PendingQuestion | null;
  requiresConversation: boolean;
}

interface Props {
  context: CompanionQuestionContext;
  drafts: Map<string, QuestionDraft>;
  onAnswer: (request: PendingQuestion, response: QuestionResponse) => Promise<boolean>;
  onInteract: (request: PendingQuestion) => Promise<boolean>;
  onOpenConversation: () => void;
  error?: string | null;
}

export function CompanionQuestion({ context, drafts, onAnswer, onInteract, onOpenConversation, error }: Props) {
  const visual = context.request?.questions.some(question => question.options.some(option => option.preview));
  if (!context.request || context.requiresConversation || visual) return <section className="companion-question min-h-0 flex-1" aria-label="Decisão no Jarvis">
    <ScrollArea className="h-full min-h-0"><div className="flex min-h-40 flex-col gap-4 px-5 pb-5 pr-7">
      <div className="flex-1 space-y-3"><p className="truncate font-mono text-[9px] text-muted-foreground">{context.projectName}</p><h2 className="break-words text-sm font-semibold">{context.title}</h2><p className="text-xs leading-5 text-muted-foreground">Esta decisão precisa do contexto completo da conversa. Abra o Jarvis para revisar e continuar.</p>{error && <p role="alert" className="text-xs text-onedark-yellow">{error}</p>}</div>
      <div className="companion-question-footer shrink-0"><Button className="w-full cursor-pointer" size="sm" onClick={onOpenConversation}><ArrowUpRight className="size-3.5" />Continuar no Jarvis</Button></div>
    </div></ScrollArea>
  </section>;
  return <Question key={`${context.conversationId}/${context.agentId ?? "root"}/${context.request.turnId}/${context.request.toolId}`} {...{ context, drafts, onAnswer, onInteract, onOpenConversation, error }} request={context.request} />;
}

function Question({ context, drafts, request, onAnswer, onInteract, onOpenConversation, error }: Props & { request: PendingQuestion }) {
  const draftKey = `${context.conversationId}/${context.agentId ?? "root"}/${request.turnId}/${request.toolId}`;
  const [draft, setDraft] = useState<QuestionDraft>(() => drafts.get(draftKey) ?? { index: 0, answers: {}, custom: {} });
  const [submitting, setSubmitting] = useState(false);
  const [pausing, setPausing] = useState(false);
  const [remaining, setRemaining] = useState<number | null>(null);
  const busy = useRef(false);
  const pauseBusy = useRef(false);
  const pauseConfirmed = useRef(false);
  const titleId = useId();
  const textId = useId();
  const question = request.questions[draft.index] ?? request.questions[0];
  const answer = draft.answers[question.id];
  const last = draft.index === request.questions.length - 1;
  const ready = request.questions.every(item => draft.answers[item.id]?.value.trim());
  const recommended = request.questions.every(item => item.options.some(option => option.recommended));
  const paused = Boolean(draft.automaticPaused);
  const deadline = recommended && !paused && !pausing ? request.deadlineAt : undefined;
  const interact = useCallback(async () => {
    if (!request.deadlineAt || paused || pauseConfirmed.current || drafts.get(draftKey)?.automaticPaused || pauseBusy.current || busy.current) return;
    pauseBusy.current = true; setPausing(true);
    try {
      if (await onInteract(request)) {
        pauseConfirmed.current = true;
        setDraft(current => { const next = { ...current, automaticPaused: true }; drafts.set(draftKey, next); return next; });
      }
    } finally { pauseBusy.current = false; setPausing(false); }
  }, [draftKey, drafts, onInteract, paused, request]);
  const update = (next: QuestionDraft) => {
    void interact();
    const updated = { ...next, automaticPaused: pauseConfirmed.current || drafts.get(draftKey)?.automaticPaused };
    drafts.set(draftKey, updated); setDraft(updated);
  };
  const submit = async (cancelled: boolean) => {
    if (busy.current || !cancelled && !ready) return;
    busy.current = true; setSubmitting(true);
    let accepted = false;
    try { accepted = await onAnswer(request, { cancelled, answers: cancelled ? [] : request.questions.map(item => draft.answers[item.id]) }); }
    finally { if (accepted) drafts.delete(draftKey); else { busy.current = false; setSubmitting(false); } }
  };
  useEffect(() => {
    if (!deadline || submitting) return;
    const tick = () => setRemaining(Math.max(0, Math.ceil((deadline - Date.now()) / 1_000)));
    const initial = window.setTimeout(tick, 0);
    const interval = window.setInterval(tick, 250);
    return () => { window.clearTimeout(initial); window.clearInterval(interval); };
  }, [deadline, submitting]);
  const next = () => {
    if (!answer?.value.trim() || submitting) return;
    if (last && ready) void submit(false);
    else update({ ...draft, index: last ? request.questions.findIndex(item => !draft.answers[item.id]?.value.trim()) : draft.index + 1 });
  };
  return <section role="region" aria-label="Perguntas do Jarvis" aria-busy={submitting} className="companion-question min-h-0 flex-1"
    onPointerDownCapture={() => { void interact(); }} onKeyDownCapture={() => { void interact(); }} onFocusCapture={() => { void interact(); }}>
    <ScrollArea role="region" aria-label="Pergunta e ações" className="h-full min-h-0">
      <div className="flex h-full min-h-[260px] flex-col">
    <div className="shrink-0 space-y-2 px-5 pb-3">
      <div className="flex items-center justify-between gap-2"><p className="truncate font-mono text-[9px] text-muted-foreground">{context.projectName}{context.agentId ? " · subagente" : ""}</p><Badge variant="outline" className="shrink-0 font-mono text-[9px]">{draft.index + 1} de {request.questions.length}</Badge></div>
      {(paused || pausing) && <p role="status" className="text-[10px] text-muted-foreground">{pausing ? "Pausando resposta automática…" : "Resposta automática pausada"}</p>}
      {deadline && <p role="status" className="flex items-center gap-1 font-mono text-[10px] text-onedark-green"><Timer className="size-3" aria-hidden="true" />{remaining === null ? "Calculando resposta automática…" : remaining > 0 ? `Resposta automática em ${remaining}s` : "Enviando resposta automática…"}</p>}
    </div>
    <ScrollArea role="region" aria-label="Opções e resposta" className="min-h-0 flex-1 px-5 pb-1">
      <div className="space-y-4 pr-2 pb-4">
        <h2 id={titleId} className="break-words text-sm leading-5 font-semibold" aria-live="polite">{question.question}</h2>
        {question.options.length > 0 && <ToggleGroup aria-labelledby={titleId} orientation="vertical" className="w-full gap-2" disabled={submitting} value={answer?.selectedLabel ? [answer.selectedLabel] : []} onValueChange={values => {
          const label = values[0];
          if (typeof label === "string") update({ ...draft, answers: { ...draft.answers, [question.id]: { id: question.id, value: label, selectedLabel: label } } });
        }}>
          {question.options.map(option => <ToggleGroupItem key={option.label} value={option.label} className="h-auto min-h-12 w-full cursor-pointer justify-start gap-2 rounded-md border border-border px-3 py-2.5 text-left whitespace-normal">
            <span className="flex min-w-0 flex-1 flex-col gap-1.5 break-words"><span className="text-xs font-medium">{option.label}</span>{option.description && <span className="text-[11px] font-normal leading-4 text-muted-foreground">{option.description}</span>}{option.recommended && <span className="text-[9px] text-onedark-green">Recomendada</span>}</span>{answer?.selectedLabel === option.label && <Check className="size-3.5 shrink-0" aria-hidden="true" />}
          </ToggleGroupItem>)}
        </ToggleGroup>}
        <div className="space-y-1.5"><label htmlFor={textId} className="text-[10px] text-muted-foreground">Sua resposta</label><Textarea id={textId} value={answer?.selectedLabel ? "" : draft.custom[question.id] ?? ""} disabled={submitting} maxLength={4000} placeholder="Ou escreva o que você prefere…" className="min-h-20 resize-none text-xs leading-5" onChange={event => {
          const value = event.target.value;
          update({ ...draft, custom: { ...draft.custom, [question.id]: value }, answers: { ...draft.answers, [question.id]: { id: question.id, value } } });
        }} /></div>
        {error && <p role="alert" className="text-xs leading-5 text-onedark-yellow">{error}</p>}
      </div>
    </ScrollArea>
    <div className="companion-question-footer flex shrink-0 flex-col gap-2 border-t border-border px-5 py-3">
      <div className="flex items-center gap-2">
        {draft.index > 0 && <Button variant="outline" size="icon-sm" aria-label="Pergunta anterior" className="shrink-0 cursor-pointer" disabled={submitting} onClick={() => update({ ...draft, index: draft.index - 1 })}><ChevronLeft className="size-3.5" /></Button>}
        <Button size="sm" className="min-w-0 flex-1 cursor-pointer" disabled={submitting || !answer?.value.trim()} onClick={next}>{last ? ready ? "Enviar respostas" : "Revisar respostas" : "Próxima pergunta"}{!last && <ChevronRight className="size-3.5" />}</Button>
      </div>
      <div className="flex items-center justify-between gap-2"><Button variant="ghost" size="sm" className="h-7 cursor-pointer text-[10px] text-muted-foreground" disabled={submitting} onClick={() => { void submit(true); }}>Cancelar</Button><Hint content="Abrir contexto completo no Jarvis"><Button variant="ghost" size="icon-xs" aria-label="Abrir conversa no Jarvis" className="cursor-pointer text-muted-foreground" onClick={onOpenConversation}><MessageCircle className="size-3.5" /></Button></Hint></div>
    </div>
      </div>
    </ScrollArea>
  </section>;
}
