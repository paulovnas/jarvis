import { ChevronRight, CircleHelp } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { questionRequestSchema, readQuestionResponse } from "@/core/questions";
import type { ToolCallItem } from "./types";
import { ExpandQuestionVisual, QuestionVisual } from "./QuestionVisual";

export function QuestionHistory({ tool }: { tool: ToolCallItem }) {
  const request = questionRequestSchema.safeParse(tool.args);
  const response = readQuestionResponse(tool.output);
  const questions = request.success ? request.data.questions : [];
  const waiting = tool.status === "running" || tool.status === "pending";
  return <Collapsible className="min-w-0 text-muted-foreground" data-testid={`question-history-${tool.id}`}>
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 max-w-full cursor-pointer justify-start gap-2 px-1 text-[11px]">
      <CircleHelp aria-hidden="true" data-icon="inline-start" className="text-onedark-cyan" />
      <span>{questions.length === 1 ? "Feita 1 pergunta" : questions.length ? `Feitas ${questions.length} perguntas` : "Perguntas não disponíveis"}</span>
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="flex flex-col gap-3 py-2 pl-7 text-sm">
      {questions.map(question => <div key={question.id} className="flex flex-col gap-1 break-words">
        <p className="text-foreground">{question.question}</p>
        <p className="whitespace-pre-wrap">{response?.answers.find(answer => answer.id === question.id)?.value ?? (response?.cancelled ? "Não respondida" : waiting ? "Aguardando resposta" : "Não respondida")}</p>
        {question.options.filter(option => option.preview && option.label === response?.answers.find(answer => answer.id === question.id)?.selectedLabel).map(option => option.preview && <div key={option.label} className="flex max-w-72 flex-col"><QuestionVisual preview={option.preview} label={option.label} /><ExpandQuestionVisual preview={option.preview} label={option.label} /></div>)}
      </div>)}
      {tool.status === "error" && !response && <p>Não foi possível concluir estas perguntas.</p>}
    </CollapsibleContent>
  </Collapsible>;
}
