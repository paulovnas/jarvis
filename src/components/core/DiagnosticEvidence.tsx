import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { CircleAlert, Copy, Database, Download, FileWarning, LoaderCircle, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { Skeleton } from "@/components/ui/skeleton";

const eventKinds = [
  "startup",
  "clean_shutdown",
  "abrupt_shutdown",
  "panic",
  "single_instance_conflict",
  "storage_failure",
  "provider_refusal",
  "provider_failure",
] as const;

const diagnosticEventSchema = z.object({
  timestamp: z.number().int().nonnegative(),
  level: z.enum(["info", "warning", "error"]),
  event: z.enum(eventKinds),
  shutdownReason: z.enum(["user_exit", "update"]).optional(),
  operation: z.string().optional(),
  correlationId: z.string().optional(),
  provider: z.string().optional(),
  category: z.string().optional(),
  httpStatus: z.number().int().optional(),
  upstreamCode: z.string().optional(),
  requestId: z.string().optional(),
}).strict();

const diagnosticSummarySchema = z.object({
  runId: z.string(),
  appVersion: z.string(),
  os: z.string(),
  arch: z.string(),
  startedAt: z.number().int().nonnegative(),
  logFiles: z.number().int().nonnegative(),
  logBytes: z.number().int().nonnegative(),
  eventCount: z.number().int().nonnegative(),
  recentEvents: z.array(diagnosticEventSchema),
  copyable: z.string(),
}).strict();

const exportResultSchema = z.object({
  path: z.string(),
  bytes: z.number().int().nonnegative(),
  events: z.number().int().nonnegative(),
}).strict();

const databaseIntegritySchema = z.object({
  healthy: z.boolean(),
  message: z.string(),
  details: z.array(z.string()),
  durationMs: z.number().int().nonnegative(),
  journalMode: z.string(),
  busyTimeoutMs: z.number().int().nonnegative(),
}).strict();

type DiagnosticSummary = z.infer<typeof diagnosticSummarySchema>;
type DiagnosticEvent = z.infer<typeof diagnosticEventSchema>;
type DatabaseIntegrity = z.infer<typeof databaseIntegritySchema>;

const eventLabels: Record<DiagnosticEvent["event"], string> = {
  startup: "Inicialização",
  clean_shutdown: "Encerramento normal",
  abrupt_shutdown: "Encerramento abrupto detectado",
  panic: "Falha interna do processo",
  single_instance_conflict: "Conflito entre instâncias",
  storage_failure: "Falha de armazenamento",
  provider_refusal: "Solicitação recusada pelo provedor",
  provider_failure: "Falha de comunicação com o provedor",
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} KB`;
  return `${(bytes / 1024 / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} MB`;
}

function errorMessage(cause: unknown, fallback: string): string {
  if (typeof cause === "string" && cause.trim()) return cause;
  if (cause && typeof cause === "object" && "message" in cause && typeof cause.message === "string") return cause.message;
  return fallback;
}

function latestIncident(summary: DiagnosticSummary): DiagnosticEvent | undefined {
  for (let index = summary.recentEvents.length - 1; index >= 0; index -= 1) {
    const event = summary.recentEvents[index];
    if (event && event.event !== "startup" && event.event !== "clean_shutdown") return event;
  }
  return undefined;
}

export function DiagnosticEvidence() {
  const [summary, setSummary] = useState<DiagnosticSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [exporting, setExporting] = useState(false);
  const [checkingIntegrity, setCheckingIntegrity] = useState(false);
  const [integrity, setIntegrity] = useState<DatabaseIntegrity | null>(null);
  const [integrityError, setIntegrityError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void invoke("get_diagnostic_summary")
      .then(value => {
        if (active) setSummary(diagnosticSummarySchema.parse(value));
      })
      .catch(cause => {
        if (active) setLoadError(errorMessage(cause, "Não foi possível carregar as evidências locais."));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, []);

  async function copySummary() {
    if (!summary) return;
    try {
      await navigator.clipboard.writeText(summary.copyable);
      toast.success("Diagnóstico copiado");
    } catch {
      toast.error("Não foi possível copiar o diagnóstico.");
    }
  }

  async function exportBundle() {
    if (!summary || exporting) return;
    const day = new Date().toISOString().slice(0, 10);
    const destination = await save({
      title: "Salvar diagnóstico do Jarvis",
      defaultPath: `Jarvis-diagnostico-${day}.zip`,
      filters: [{ name: "Diagnóstico do Jarvis", extensions: ["zip"] }],
    });
    if (!destination) return;
    setExporting(true);
    try {
      const result = exportResultSchema.parse(await invoke("export_diagnostic_bundle", { path: destination }));
      toast.success("Pacote de diagnóstico salvo", { description: `${formatBytes(result.bytes)} · ${result.events} eventos` });
    } catch (cause) {
      toast.error(errorMessage(cause, "Não foi possível exportar o diagnóstico."));
    } finally {
      setExporting(false);
    }
  }

  async function checkIntegrity() {
    if (checkingIntegrity) return;
    setCheckingIntegrity(true);
    setIntegrityError(null);
    try {
      const result = databaseIntegritySchema.parse(await invoke("check_database_integrity"));
      setIntegrity(result);
      if (result.healthy) toast.success("Banco de dados íntegro");
      else toast.error("O banco de dados precisa de atenção.");
    } catch (cause) {
      const message = errorMessage(cause, "Não foi possível verificar o banco de dados.");
      setIntegrityError(message);
      toast.error(message);
    } finally {
      setCheckingIntegrity(false);
    }
  }

  const incident = summary ? latestIncident(summary) : undefined;
  return <Card className="instrument-panel mt-5 gap-0 py-0">
    <CardContent className="space-y-4 p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <span className="flex size-8 shrink-0 items-center justify-center rounded-md border border-onedark-cyan/25 bg-onedark-cyan/10 text-onedark-cyan"><FileWarning aria-hidden="true" className="size-4" /></span>
          <div className="min-w-0"><h2 className="text-sm font-medium">Evidências de execução</h2><p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">Logs locais sanitizados. Prompts, respostas, chaves e corpos HTTP não são registrados.</p></div>
        </div>
        {summary && <Badge variant="outline" className="rounded-md font-mono text-[9px] text-muted-foreground">{summary.eventCount} eventos · {formatBytes(summary.logBytes)}</Badge>}
      </div>

      {loading && <div role="status" aria-label="Carregando evidências" className="grid gap-2 sm:grid-cols-3"><Skeleton className="h-12" /><Skeleton className="h-12" /><Skeleton className="h-12" /></div>}
      {loadError && <div role="alert" className="flex items-start gap-2 rounded-md border border-destructive/25 bg-destructive/5 p-3 text-xs text-destructive"><CircleAlert aria-hidden="true" className="mt-0.5 size-3.5 shrink-0" /><span>{loadError}</span></div>}
      {summary && <>
        <dl className="grid gap-2 text-[11px] sm:grid-cols-3">
          <div className="rounded-md border border-border bg-sidebar p-3"><dt className="micro-label text-muted-foreground">Versão</dt><dd className="mt-1 font-mono">{summary.appVersion}</dd></div>
          <div className="rounded-md border border-border bg-sidebar p-3"><dt className="micro-label text-muted-foreground">Sistema</dt><dd className="mt-1 font-mono">{summary.os} · {summary.arch}</dd></div>
          <div className="rounded-md border border-border bg-sidebar p-3"><dt className="micro-label text-muted-foreground">Execução</dt><Hint content={summary.runId}><dd className="mt-1 truncate font-mono">{summary.runId}</dd></Hint></div>
        </dl>
        <div className="flex items-start gap-2 rounded-md border border-border bg-sidebar p-3 text-xs">
          {incident ? <CircleAlert aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-onedark-yellow" /> : <ShieldCheck aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-onedark-green" />}
          <div><p className="font-medium">{incident ? eventLabels[incident.event] : "Nenhum incidente recente"}</p><p className="mt-1 font-mono text-[10px] text-muted-foreground">{incident ? new Date(incident.timestamp).toLocaleString("pt-BR") : `Execução iniciada em ${new Date(summary.startedAt).toLocaleString("pt-BR")}`}</p></div>
        </div>
      </>}

      <div className="flex flex-wrap items-start gap-3 rounded-md border border-border bg-sidebar p-3">
        <span className="flex size-8 shrink-0 items-center justify-center rounded-md border border-onedark-green/25 bg-onedark-green/10 text-onedark-green"><Database aria-hidden="true" className="size-4" /></span>
        <div className="min-w-[12rem] flex-1" aria-live="polite">
          <p className="text-xs font-medium">Integridade do banco de dados</p>
          <p className={`mt-1 text-[11px] leading-relaxed ${integrity && !integrity.healthy ? "text-destructive" : "text-muted-foreground"}`}>{integrity?.message ?? "Execute uma verificação rápida e segura da estrutura do SQLite."}</p>
          {integrity && <p className="mt-1 font-mono text-[9px] text-muted-foreground">Modo {integrity.journalMode.toUpperCase()} · espera de {(integrity.busyTimeoutMs / 1_000).toLocaleString("pt-BR", { maximumFractionDigits: 1 })}s · {integrity.durationMs}ms</p>}
          {integrity && !integrity.healthy && <ul className="mt-2 space-y-1 font-mono text-[9px] text-destructive">{integrity.details.map((detail, index) => <li key={`${index}-${detail}`}>{detail}</li>)}</ul>}
          {integrityError && <p role="alert" className="mt-2 text-[11px] text-destructive">{integrityError}</p>}
        </div>
        <Button type="button" size="sm" variant="outline" className="cursor-pointer" disabled={checkingIntegrity} onClick={() => void checkIntegrity()}>
          {checkingIntegrity ? <LoaderCircle aria-hidden="true" className="size-3.5 animate-spin" /> : <ShieldCheck aria-hidden="true" className="size-3.5" />}
          {checkingIntegrity ? "Verificando…" : "Verificar banco"}
        </Button>
      </div>

      <div className="flex flex-wrap justify-end gap-2 border-t border-border pt-3">
        <Button type="button" size="sm" variant="ghost" className="cursor-pointer" disabled={!summary || loading || exporting} onClick={() => void copySummary()}><Copy aria-hidden="true" className="size-3.5" />Copiar diagnóstico</Button>
        <Button type="button" size="sm" variant="outline" className="cursor-pointer" disabled={!summary || loading || exporting} onClick={() => void exportBundle()}><Download aria-hidden="true" className="size-3.5" />{exporting ? "Exportando…" : "Exportar pacote"}</Button>
      </div>
    </CardContent>
  </Card>;
}
