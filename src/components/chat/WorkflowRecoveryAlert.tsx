import { useState } from "react";
import { RotateCcw, ShieldAlert } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { WorkflowRecovery } from "@/core/workflow";

export function WorkflowRecoveryAlert({
  recovery,
  onResume,
}: {
  recovery: WorkflowRecovery;
  onResume: () => Promise<boolean>;
}) {
  const [resuming, setResuming] = useState(false);
  const tools = [...new Set(recovery.uncertainActions.map(action => action.tool))];

  return <Alert className="mb-3 border-onedark-yellow/35 bg-onedark-yellow/6 text-foreground">
    <ShieldAlert aria-hidden="true" className="text-onedark-yellow" />
    <AlertTitle>Fluxo interrompido disponível para retomada</AlertTitle>
    <AlertDescription className="space-y-3 text-xs text-muted-foreground">
      <p>
        {recovery.affectedAgents === 1
          ? "O agente principal será reconstruído a partir do último checkpoint."
          : `${recovery.affectedAgents} agentes serão reconstruídos a partir dos últimos checkpoints.`}
        {recovery.uncertainActions.length > 0
          ? ` ${recovery.uncertainActions.length} ${recovery.uncertainActions.length === 1 ? "ação ficou" : "ações ficaram"} com resultado incerto e precisarão ser conferidas antes de qualquer nova alteração.`
          : " O Jarvis exigirá uma leitura de verificação antes de qualquer nova alteração."}
      </p>
      {tools.length > 0 && <div aria-label="Ferramentas com resultado incerto" className="flex flex-wrap gap-1.5">
        {tools.slice(0, 6).map(tool => <Badge key={tool} variant="outline" className="border-onedark-yellow/25 bg-background/40 font-mono text-[9px] text-onedark-yellow">{tool}</Badge>)}
        {tools.length > 6 && <Badge variant="outline" className="border-border font-mono text-[9px]">+{tools.length - 6}</Badge>}
      </div>}
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="cursor-pointer gap-2 border-onedark-yellow/30 text-onedark-yellow hover:bg-onedark-yellow/10 hover:text-onedark-yellow"
        disabled={resuming}
        onClick={() => {
          setResuming(true);
          void onResume().finally(() => setResuming(false));
        }}
      >
        <RotateCcw aria-hidden="true" className={resuming ? "animate-spin motion-reduce:animate-none" : ""} />
        {resuming ? "Retomando…" : "Retomar fluxo"}
      </Button>
    </AlertDescription>
  </Alert>;
}
