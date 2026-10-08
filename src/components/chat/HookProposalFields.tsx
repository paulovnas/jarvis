import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { HOOK_EVENT_LABELS } from "@/core/hooks";
import type { PendingAuthoring } from "@/core/authoring";

type HookTarget = Extract<PendingAuthoring["target"], { kind: "hook" }>;

export function HookProposalFields({ target }: { target: HookTarget }) {
  const hook = target.after ?? target.before;
  if (!hook) return null;
  return <Card className="gap-0 overflow-hidden border-border bg-card/80 p-0">
    <CardHeader className="gap-2 border-b border-border p-4">
      <CardTitle className="break-words">{hook.name}</CardTitle>
      <div className="flex flex-wrap gap-2"><Badge variant="outline">{HOOK_EVENT_LABELS[hook.event].label}</Badge><Badge variant="outline" className="text-onedark-yellow">{target.after ? hook.enabled ? "Ativado" : "Desativado" : "Será removido"}</Badge></div>
    </CardHeader>
    <CardContent className="space-y-3 p-4">
      <section><p className="micro-label mb-1 text-muted-foreground">Comando local</p><pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all rounded-md border border-border bg-background p-3 font-mono text-xs">{hook.command}</pre></section>
      <div className="grid gap-3 sm:grid-cols-2">
        <section><p className="micro-label mb-1 text-muted-foreground">Matcher</p><p className="break-all font-mono text-xs">{hook.matcher || "Sem filtro"}</p></section>
        <section><p className="micro-label mb-1 text-muted-foreground">Tempo limite</p><p className="font-mono text-xs">{hook.timeoutSeconds} s</p></section>
      </div>
      <p className="text-xs leading-5 text-muted-foreground">A alteração valerá nos próximos turnos das conversas de projeto. Este comando não será executado durante a aprovação.</p>
    </CardContent>
  </Card>;
}
