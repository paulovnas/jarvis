import { BrainCircuit, ChevronRight, MessageSquare } from "lucide-react";
import { Fragment } from "react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import { ToolCallCard } from "./ToolCallCard";
import type { AssistantWorkData } from "./types";

export function AssistantWorkCollapse({ work, isStreaming = false }: { work: AssistantWorkData; isStreaming?: boolean }) {
  const allTools = work.steps.flatMap(step => step.tools);
  const tools = allTools.filter(tool => tool.name !== "ask_user");
  const waiting = allTools.some(tool => tool.name === "ask_user" && (tool.status === "running" || tool.status === "pending"));
  const failures = tools.filter(tool => tool.status === "error").length;
  return (
    <Collapsible render={<section aria-label="Processamento do Jarvis" />} className="min-w-0 text-muted-foreground">
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 max-w-full cursor-pointer justify-start gap-2 px-1 text-left">
        {isStreaming ? <Spinner aria-label="Em execução" className="motion-reduce:animate-none" data-icon="inline-start" /> : <BrainCircuit aria-hidden="true" data-icon="inline-start" />}
        <span className="truncate">{isStreaming ? waiting ? "Aguardando sua resposta" : "Pensando e executando…" : `Trabalhou por ${work.durationSeconds}s`}</span>
        {tools.length > 0 && <span className="shrink-0 text-muted-foreground">· {tools.length} {tools.length === 1 ? "ação" : "ações"}</span>}
        {failures > 0 && <span className="sr-only">{failures} {failures === 1 ? "ação não concluída" : "ações não concluídas"}</span>}
        <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      </CollapsibleTrigger>
      <CollapsibleContent className="ml-2 flex min-w-0 flex-col gap-0.5 border-l border-border py-1 pl-3">
        {work.steps.map((step, index) => <Fragment key={index}>
          {(step.thinking || step.commentary) && <Collapsible>
            <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group max-w-full cursor-pointer justify-start gap-2 px-1">
              {step.thinking ? <BrainCircuit aria-hidden="true" data-icon="inline-start" /> : <MessageSquare aria-hidden="true" data-icon="inline-start" />}
              <span className="truncate">{step.thinking ? "Resumo de raciocínio do provedor" : "Observações do agente"} · {index + 1}</span>
              <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
            </CollapsibleTrigger>
            <CollapsibleContent className="flex flex-col gap-2 py-2 pl-6 text-sm leading-relaxed">
              {step.thinking && <p className="whitespace-pre-wrap break-words">{step.thinking}</p>}
              {step.commentary && <p className="whitespace-pre-wrap break-words text-foreground">{step.commentary}</p>}
            </CollapsibleContent>
          </Collapsible>}
          {step.tools.filter(tool => tool.name !== "ask_user").map(tool => <ToolCallCard key={tool.id} tool={tool} />)}
        </Fragment>)}
        {work.steps.length === 0 && <p className="py-1 text-xs">Conectando ao provedor…</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}
