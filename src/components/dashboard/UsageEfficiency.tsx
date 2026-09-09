import { Database, Layers3, ScanEye } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { emptyEfficiency, number, type ProjectMetrics } from "@/core/dashboard";

export function UsageEfficiency({ metrics }: { metrics: ProjectMetrics["metrics"] }) {
  const e = metrics.efficiency ?? emptyEfficiency;
  const requests = metrics.measuredSteps + e.auxiliaryRequests;
  const hitRate = e.cacheReadInputTokens > 0 ? e.cacheReadTokens / e.cacheReadInputTokens * 100 : 0;
  const reduction = e.originalBytes > 0 ? Math.max(0, 1 - e.retainedBytes / e.originalBytes) * 100 : 0;
  return <Card className="dashboard-card gap-3">
    <CardHeader><CardTitle className="text-sm">Eficiência</CardTitle></CardHeader>
    <CardContent className="grid gap-5 @3xl:grid-cols-3">
      <section aria-label="Cache do provedor" className="min-w-0 space-y-3">
        <p className="micro-label flex items-center gap-2 text-muted-foreground"><Database className="size-3.5 text-onedark-green" />Cache do provedor</p>
        <p className="font-mono text-xl text-onedark-green">{e.cacheReadRequests ? `${number(hitRate)}%` : "Não informado"}</p>
        <Progress value={hitRate} className="h-1" aria-label="Entrada reaproveitada do cache" />
        <Line label="Tokens reutilizados" value={e.cacheReadRequests ? number(e.cacheReadTokens) : "—"} />
        <Line label="Tokens gravados" value={e.cacheWriteRequests ? number(e.cacheWriteTokens) : "—"} />
        <p className="text-[11px] text-muted-foreground">{e.cacheReadRequests ? `${number(e.cacheReadRequests)}/${number(requests)} chamadas com leitura de cache informada` : "Histórico sem medição de cache."}</p>
      </section>
      <section aria-label="Redução pelo Context-mode" className="min-w-0 space-y-3">
        <p className="micro-label flex items-center gap-2 text-muted-foreground"><Layers3 className="size-3.5 text-onedark-cyan" />Context-mode automático</p>
        <p className="font-mono text-xl text-onedark-cyan">{number(e.indexedOutputs)} <span className="text-xs text-muted-foreground">resultados indexados</span></p>
        <Progress value={reduction} className="h-1" aria-label="Redução dos resultados enviados ao modelo" />
        <Line label="Redução de conteúdo" value={e.indexedOutputs ? `${number(reduction)}%` : "—"} />
        <Line label="Consultas automáticas" value={number(e.contextSearches)} />
        <Line label="Loops orientados" value={number(e.loopSteers)} />
        <Line label="Repetições bloqueadas" value={number(e.loopAvoidedCalls)} />
        <Line label="Original → enviado" value={e.indexedOutputs ? `${bytes(e.originalBytes)} → ${bytes(e.retainedBytes)}` : "—"} />
        <p className="text-[11px] text-muted-foreground">Medição em bytes; conteúdo completo disponível para consulta.</p>
      </section>
      <section aria-label="Consumo de Vision e Web Search" className="min-w-0 space-y-3">
        <p className="micro-label flex items-center gap-2 text-muted-foreground"><ScanEye className="size-3.5 text-onedark-purple" />Vision + Web Search</p>
        <p className="font-mono text-xl">{number(e.auxiliaryRequests)} <span className="text-xs text-muted-foreground">chamadas medidas</span></p>
        <Line label="Entrada" value={number(e.auxiliaryInputTokens)} />
        <Line label="Saída" value={number(e.auxiliaryOutputTokens)} />
        <p className="text-[11px] text-muted-foreground">Incluídas nos tokens acumulados.</p>
      </section>
    </CardContent>
  </Card>;
}

function bytes(value: number) { return value >= 1_048_576 ? `${number(value / 1_048_576)} MiB` : `${number(value / 1024)} KiB`; }
function Line({ label, value }: { label: string; value: string }) {
  return <div className="flex justify-between gap-3 text-xs"><span className="text-muted-foreground">{label}</span><span className="font-mono tabular-nums">{value}</span></div>;
}
