import { useEffect, useId, useRef, useState } from "react";
import { Check, ChevronLeft, ChevronRight, PencilLine, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/TextInput";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { PendingQuestion, QuestionDraft, QuestionResponse } from "@/core/questions";
import { ExpandQuestionVisual, QuestionVisual } from "./QuestionVisual";

export function QuestionCard({ request, drafts, draftKey, onAnswer }: {
  request: PendingQuestion;
  drafts: Map<string, QuestionDraft>;
  draftKey: string;
  onAnswer: (request: PendingQuestion, response: QuestionResponse) => Promise<boolean>;
}) {
  const [draft, setDraft] = useState<QuestionDraft>(() => drafts.get(draftKey) ?? { index: 0, answers: {}, custom: {} });
  const [pending, setPending] = useState(false);
  const submitting = useRef(false);
  const title = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const inputId = useId();
  const question = request.questions[draft.index];
  const answer = draft.answers[question.id];
  const last = draft.index === request.questions.length - 1;
  const ready = request.questions.every(item => draft.answers[item.id]?.value.trim());
  const otherAnswersReady = request.questions.every(item => item.id === question.id || draft.answers[item.id]?.value.trim());
  const update = (next: QuestionDraft) => { drafts.set(draftKey, next); setDraft(next); };
  const navigate = (index: number) => { update({ ...draft, index }); title.current?.focus(); };
  useEffect(() => { title.current?.focus(); }, []);
  const submit = async (cancelled: boolean) => {
    if (submitting.current || (!cancelled && !ready)) return;
    submitting.current = true; setPending(true);
    const accepted = await onAnswer(request, { cancelled, answers: cancelled ? [] : request.questions.map(item => draft.answers[item.id]) });
    if (accepted) drafts.delete(draftKey);
    else { submitting.current = false; setPending(false); }
  };
  return <Card size="sm" role="region" aria-label="Perguntas do Jarvis" aria-busy={pending} className="mx-auto mb-3 max-w-4xl rounded-lg" onKeyDown={event => {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); void submit(true); }
  }}>
    <CardHeader className="flex flex-row items-start justify-between gap-3">
      <CardTitle id={titleId} ref={title} tabIndex={-1} className="min-w-0 flex-1 break-words outline-none" aria-live="polite">{question.question}</CardTitle>
      <div className="flex shrink-0 items-center gap-1">
        {request.questions.length > 1 && <>
          <Button variant="ghost" size="icon-xs" className="cursor-pointer" aria-label="Pergunta anterior" disabled={pending || draft.index === 0} onClick={() => navigate(draft.index - 1)}><ChevronLeft /></Button>
          <span className="text-xs tabular-nums text-muted-foreground">{draft.index + 1} de {request.questions.length}</span>
          <Button variant="ghost" size="icon-xs" className="cursor-pointer" aria-label="Próxima pergunta" disabled={pending || last} onClick={() => navigate(draft.index + 1)}><ChevronRight /></Button>
        </>}
        <Button variant="ghost" size="icon-xs" className="cursor-pointer" aria-label="Cancelar perguntas" disabled={pending} onClick={() => { void submit(true); }}><X /></Button>
      </div>
    </CardHeader>
    <CardContent>
      <form className="flex flex-col gap-3" onSubmit={event => {
        event.preventDefault();
        if (pending || !answer?.value.trim()) return;
        if (last) { if (ready) void submit(false); else navigate(request.questions.findIndex(item => !draft.answers[item.id]?.value.trim())); }
        else navigate(draft.index + 1);
      }}>
        {question.options.length > 0 && <ToggleGroup orientation="vertical" aria-labelledby={titleId} className={`max-h-[42vh] w-full overflow-y-auto ${question.options.some(option => option.preview) ? "grid grid-cols-1 items-start gap-2 sm:grid-cols-2" : ""}`} disabled={pending}
          value={answer?.selectedLabel ? [answer.selectedLabel] : []}
          onValueChange={values => {
            const label = values[0];
            if (typeof label === "string") update({ ...draft, answers: { ...draft.answers, [question.id]: { id: question.id, value: label, selectedLabel: label } } });
          }}>
          {question.options.map((option, index) => <div key={`${question.id}-${index}`} className="flex w-full min-w-0 flex-col"><ToggleGroupItem value={option.label} className="h-auto min-h-10 w-full cursor-pointer justify-start gap-3 px-3 py-2 text-left whitespace-normal">
            <Badge variant="outline" className="size-6 shrink-0 justify-center rounded-full p-0" aria-hidden="true">{index + 1}</Badge>
            <span className="flex min-w-0 flex-1 flex-col gap-1.5 break-words"><span>{option.label}</span>{option.preview && <span className="block w-full max-w-72"><QuestionVisual preview={option.preview} label={option.label} /></span>}{option.description && <span className="text-xs font-normal text-muted-foreground">{option.description}</span>}</span>
            {answer?.selectedLabel === option.label && <Check aria-hidden="true" data-icon="inline-end" />}
          </ToggleGroupItem>{option.preview && <ExpandQuestionVisual preview={option.preview} label={option.label} />}</div>)}
        </ToggleGroup>}
        <div className="flex items-center gap-2">
          <PencilLine aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
          <Field className="min-w-0 flex-1">
            <FieldLabel className="sr-only" htmlFor={inputId}>Sua resposta</FieldLabel>
            <Input id={inputId} placeholder="Escreva sua resposta…" value={answer?.selectedLabel ? "" : draft.custom[question.id] ?? ""} disabled={pending} maxLength={4000}
              onChange={event => {
                const value = event.target.value;
                update({ ...draft, custom: { ...draft.custom, [question.id]: value }, answers: { ...draft.answers, [question.id]: { id: question.id, value } } });
              }} />
          </Field>
          <Button type="submit" size="sm" className="shrink-0 cursor-pointer rounded-full" disabled={pending || !answer?.value.trim()}>{last ? otherAnswersReady ? "Enviar respostas" : "Revisar" : "Avançar"}</Button>
        </div>
      </form>
    </CardContent>
  </Card>;
}
