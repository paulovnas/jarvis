import { lazy, Suspense, useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { LockKeyhole } from "lucide-react";
import { AGENT_ICONS } from "@/components/agents/agent-presentation";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { DocumentSkeleton } from "@/components/layout/LoadingSkeletons";
import { FLOW_LABELS, ROLE_COLORS, ROLE_LABELS, type Workflow, type WorkflowAgent } from "@/core/workflow";
import { libraryError } from "@/core/library";

const Markdown = lazy(() => import("@/components/chat/ChatMarkdown"));
const sectionsSchema = z.array(z.object({ title: z.string(), content: z.string() })).min(1);
type Sections = z.infer<typeof sectionsSchema>;

export function AgentInstructionsDialog({ flow, role, children }: { flow: Workflow; role: WorkflowAgent["role"]; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<{ sections?: Sections; error?: string } | null>(null);
  useEffect(() => {
    if (!open) return;
    let active = true;
    void invoke("get_agent_instructions", { flow, role })
      .then(value => { if (active) setResult({ sections: sectionsSchema.parse(value) }); })
      .catch(cause => { if (active) setResult({ error: libraryError(cause, "Não foi possível carregar as instruções.") }); });
    return () => { active = false; };
  }, [open, flow, role, attempt]);
  const Icon = AGENT_ICONS[role];
  return <Dialog open={open} onOpenChange={setOpen}>
    {children}
    <DialogContent className="dark flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl motion-reduce:transition-none">
      <DialogHeader className="shrink-0 border-b bg-card/80 px-6 py-5 pr-12">
        <DialogTitle className="flex items-center gap-3 text-base">
          <span aria-hidden="true" className="flex size-9 shrink-0 items-center justify-center rounded-md border" style={{ color: ROLE_COLORS[role], borderColor: `${ROLE_COLORS[role]}40`, backgroundColor: `${ROLE_COLORS[role]}10` }}><Icon className="size-4" /></span>
          <span>{ROLE_LABELS[role]}</span>{" "}<Badge variant="outline" className="text-[10px] text-muted-foreground">{FLOW_LABELS[flow]}</Badge>
        </DialogTitle>
        <DialogDescription className="flex items-center gap-1.5 pl-12 text-xs"><LockKeyhole aria-hidden="true" className="size-3" />Instruções fixas · Somente leitura</DialogDescription>
      </DialogHeader>
      {result?.error ? <div className="space-y-3 p-6"><p role="alert" className="text-sm text-destructive">{result.error}</p><Button variant="outline" className="cursor-pointer" onClick={() => { setResult(null); setAttempt(value => value + 1); }}>Tentar novamente</Button></div>
        : !result?.sections ? <div className="p-6"><DocumentSkeleton label="Carregando instruções do agente" /></div>
        : <ScrollArea className="min-h-0 flex-1 [&>[data-slot=scroll-area-viewport]]:max-h-[65dvh]">
          <article aria-label={`Instruções de ${ROLE_LABELS[role]}`} className="min-w-0 space-y-7 px-6 py-5">
            {result.sections.map(section => <section key={section.title} className="min-w-0">
              <h2 className="mb-3 border-b border-border/60 pb-2 text-sm font-semibold text-foreground">{section.title}</h2>
              <div className="min-w-0 break-words text-[13px] leading-6 text-muted-foreground [&_h1]:my-3 [&_h1]:text-base [&_h2]:mt-4 [&_h2]:mb-2 [&_h2]:text-sm [&_h3]:mt-3 [&_h3]:font-semibold [&_p]:my-3 [&_p:first-child]:mt-0 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-1 [&_strong]:font-medium [&_strong]:text-foreground [&_pre]:my-3 [&_pre]:overflow-x-auto [&_code]:font-mono [&_code]:text-xs [&_blockquote]:border-l-2 [&_blockquote]:border-primary/40 [&_blockquote]:pl-4 [&_table]:block [&_table]:overflow-x-auto [&_td]:border [&_td]:p-2 [&_th]:border [&_th]:p-2">
                <Suspense fallback={<DocumentSkeleton />}><Markdown content={section.content} /></Suspense>
              </div>
            </section>)}
          </article>
        </ScrollArea>}
    </DialogContent>
  </Dialog>;
}
