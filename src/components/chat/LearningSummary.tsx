import { Brain, ChevronRight } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { LESSON_STATUS, type ProjectLesson } from "@/core/project-learning";

export function LearningSummary({ lessons }: { lessons: ProjectLesson[] }) {
  if (lessons.length === 0) return null;
  return <Collapsible className="mt-1 min-w-0">
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 cursor-pointer gap-2 px-1 text-[11px] text-muted-foreground hover:bg-transparent hover:text-foreground">
      <Brain aria-hidden="true" data-icon="inline-start" />
      <span>Aprendizados registrados</span>
      <span className="font-mono text-[10px] tabular-nums">· {lessons.length}</span>
      <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="ml-2 border-l border-border/60 py-2 pl-3">
      <ul aria-label="Aprendizados desta interação" className="flex min-w-0 flex-col gap-4">
        {lessons.map(lesson => <li key={lesson.id} className="flex min-w-0 flex-col gap-1.5 text-xs leading-relaxed">
          <p className="whitespace-pre-wrap break-words text-foreground">{lesson.content}</p>
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="outline" className="text-[10px] font-normal text-muted-foreground">{LESSON_STATUS[lesson.status]}</Badge>
            <span className="break-all font-mono text-[10px] text-muted-foreground">{lesson.scope === "." ? "Projeto inteiro" : lesson.scope}</span>
          </div>
          {lesson.check && <p className="whitespace-pre-wrap break-words text-muted-foreground"><span className="font-medium">Como verificar: </span>{lesson.check}</p>}
          {lesson.status === "suggested" && <p className="text-muted-foreground">Ainda não usado pelos agentes. Ative em Opções → Aprendizados após revisar.</p>}
        </li>)}
      </ul>
    </CollapsibleContent>
  </Collapsible>;
}
