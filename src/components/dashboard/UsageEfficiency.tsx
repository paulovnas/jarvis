import { ChevronDown, Database, Files, Layers3 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { emptyEfficiency, number, type ProjectMetrics } from "@/core/dashboard";

const reduction = (original: number, retained: number) => original > 0 ? Math.max(0, Math.min(100, (1 - retained / original) * 100)) : null;

export function UsageEfficiency({ metrics }: { metrics: ProjectMetrics["metrics"] }) {
  const e = metrics.efficiency ?? emptyEfficiency;
  const requests = metrics.measuredSteps + e.auxiliaryRequests;
  const hitRate = e.cacheReadRequests > 0 && e.cacheReadInputTokens > 0 ? Math.min(100, e.cacheReadTokens / e.cacheReadInputTokens * 100) : null;
  const outputReduction = e.indexedOutputs > 0 ? reduction(e.originalBytes, e.retainedBytes) : null;
  const readReduction = e.localReadReuses > 0 ? reduction(e.localReadOriginalBytes, e.localReadRetainedBytes) : null;

  return <Card className="dashboard-card gap-5">
    <CardHeader>
      <CardTitle>Uso inteligente do contexto</CardTitle>
      <CardDescription>Menos conteúdo repetido nas conversas. Dados acumulados deste projeto.</CardDescription>
    </CardHeader>
    <CardContent className="flex flex-col gap-5">
      <div className="grid gap-6 @3xl:grid-cols-3">
        <section aria-label="Cache do provedor" className="flex min-w-0 flex-col gap-2">
          <p className="flex items-center gap-2 text-sm font-medium"><Database className="size-4 text-onedark-green" aria-hidden="true" />Contexto reaproveitado</p>
          <p className="font-mono text-2xl tabular-nums">{hitRate === null ? <span className="font-sans text-base text-muted-foreground">Sem medição</span> : `${number(hitRate)}%`}</p>
          {hitRate !== null && <Progress value={hitRate} className="h-1" aria-label="Entrada reaproveitada do cache" />}
          <p className="text-xs leading-relaxed text-muted-foreground">{hitRate === null ? "O provedor ainda não informou dados suficientes de cache." : "Da entrada medida já era conhecida pelo provedor e foi reutilizada."}</p>
          {e.cacheReadRequests > 0 && <p className="text-xs text-muted-foreground">Cache informado em {number(e.cacheReadRequests)} de {number(requests)} chamadas.</p>}
        </section>
        <section aria-label="Redução pelo Context-mode" className="flex min-w-0 flex-col gap-2">
          <p className="flex items-center gap-2 text-sm font-medium"><Layers3 className="size-4 text-onedark-cyan" aria-hidden="true" />Resultados resumidos</p>
          <p className="font-mono text-2xl tabular-nums">{number(e.indexedOutputs)}</p>
          <p className="text-xs leading-relaxed text-muted-foreground">Resultados longos de ferramentas enviados de forma compacta. A IA pode consultar o conteúdo completo.</p>
          {outputReduction !== null && <p className="text-xs text-onedark-cyan">{number(outputReduction)}% menos conteúdo nesses resultados.</p>}
        </section>
        <section aria-label="Releituras locais" className="flex min-w-0 flex-col gap-2">
          <p className="flex items-center gap-2 text-sm font-medium"><Files className="size-4 text-onedark-yellow" aria-hidden="true" />Leituras reaproveitadas</p>
          <p className="font-mono text-2xl tabular-nums">{number(e.localReadReuses)}</p>
          <p className="text-xs leading-relaxed text-muted-foreground">Leituras de arquivos reutilizadas após confirmar que o conteúdo não mudou.</p>
          {readReduction !== null && <p className="text-xs text-onedark-yellow">{number(readReduction)}% menos conteúdo nas releituras.</p>}
        </section>
      </div>
      <Separator />
      <Collapsible>
        <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group w-full justify-between">
          Ver medições detalhadas<ChevronDown data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-180 motion-reduce:transition-none" />
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="grid gap-6 pt-5 @3xl:grid-cols-2">
            <section aria-label="Medições de cache e conteúdo" className="flex min-w-0 flex-col gap-3">
              <h3 className="text-sm font-medium">Cache e conteúdo enviado</h3>
              <Line label="Tokens reutilizados pelo provedor" value={e.cacheReadRequests ? number(e.cacheReadTokens) : "Não informado"} />
              <Line label="Tokens gravados no cache" value={e.cacheWriteRequests ? number(e.cacheWriteTokens) : "Não informado"} />
              <Line label="Context-mode · original → enviado" value={e.indexedOutputs ? `${bytes(e.originalBytes)} → ${bytes(e.retainedBytes)}` : "Sem uso registrado"} />
              <Line label="Releituras · original → enviado" value={e.localReadReuses ? `${bytes(e.localReadOriginalBytes)} → ${bytes(e.localReadRetainedBytes)}` : "Sem uso registrado"} />
              <Line label="Consultas ao conteúdo completo" value={number(e.contextSearches)} />
            </section>
            <section aria-label="Outras atividades medidas" className="flex min-w-0 flex-col gap-3">
              <h3 className="text-sm font-medium">Outras atividades</h3>
              <Line label="Orientações para evitar repetição" value={number(e.loopSteers)} />
              <Line label="Chamadas repetidas evitadas" value={number(e.loopAvoidedCalls)} />
              <Line label="Visão e busca na web · chamadas medidas" value={number(e.auxiliaryRequests)} />
              <Line label="Visão e busca na web · tokens de entrada" value={number(e.auxiliaryInputTokens)} />
              <Line label="Visão e busca na web · tokens de saída" value={number(e.auxiliaryOutputTokens)} />
              <p className="text-xs leading-relaxed text-muted-foreground">Visão e busca já estão incluídas nos tokens acumulados do projeto.</p>
            </section>
          </div>
          <p className="mt-5 text-xs leading-relaxed text-muted-foreground">Reduções de conteúdo são medidas em bytes. Não representam uma estimativa de dinheiro ou tempo economizado. O cache considera apenas chamadas em que o provedor informou a medição.</p>
        </CollapsibleContent>
      </Collapsible>
    </CardContent>
  </Card>;
}

function bytes(value: number) { return value < 1024 ? `${number(value)} B` : value >= 1_048_576 ? `${number(value / 1_048_576)} MiB` : `${number(value / 1024)} KiB`; }
function Line({ label, value }: { label: string; value: string }) {
  return <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 text-xs"><span className="text-muted-foreground">{label}</span><span className="font-mono tabular-nums">{value}</span></div>;
}
