import { useCallback, useEffect, useRef, useState } from "react";
import { FileClock, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent } from "@/components/ConfirmationDialogContent";
import { clearDiagnosticLogs, getDiagnosticLogStatus, type DiagnosticLogStatus } from "@/core/diagnostics";
import { libraryError } from "@/core/library";

function size(bytes: number) {
  const unit = bytes < 1024 ? 0 : bytes < 1024 * 1024 ? 1 : 2;
  return `${(bytes / 1024 ** unit).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} ${["B", "KB", "MB"][unit]}`;
}

export function DiagnosticLogSettings() {
  const [status, setStatus] = useState<DiagnosticLogStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [clearing, setClearing] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const busy = useRef(false);
  const refresh = useCallback(async () => {
    if (busy.current) return;
    busy.current = true; setLoading(true); setError(null);
    try { setStatus(await getDiagnosticLogStatus()); }
    catch (cause) { setStatus(null); setError(libraryError(cause, "Não foi possível analisar os logs.")); }
    finally { busy.current = false; setLoading(false); }
  }, []);
  useEffect(() => {
    let active = true;
    void getDiagnosticLogStatus().then(value => { if (active) setStatus(value); })
      .catch(cause => { if (active) setError(libraryError(cause, "Não foi possível analisar os logs.")); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);
  const clear = async () => {
    if (busy.current || !status) return;
    busy.current = true; setClearing(true); setError(null);
    try {
      const result = await clearDiagnosticLogs();
      setStatus(result.status);
      toast.success(result.freedBytes ? `${size(result.freedBytes)} liberados dos logs internos` : "Os logs internos já estavam vazios");
    } catch (cause) {
      setStatus(null);
      setError(libraryError(cause, "A limpeza pode estar parcial. Analise novamente antes de tentar."));
    } finally { busy.current = false; setClearing(false); setConfirm(false); }
  };
  return <section role="region" aria-label="Logs internos" className="flex min-w-0 flex-col gap-3 p-4">
    <div className="settings-row border-0! p-0!">
      <div className="settings-row-copy"><h3 className="flex items-center gap-2 text-sm font-medium"><FileClock aria-hidden="true" className="size-4 text-muted-foreground" />Logs internos</h3><p className="settings-row-description">Erros, avisos e diagnósticos persistentes do aplicativo e do harness. A coleta continua após a limpeza.</p></div>
      <Button variant="outline" size="sm" className="cursor-pointer" disabled={loading || clearing || !status?.files} onClick={() => setConfirm(true)}><Trash2 />{clearing ? "Limpando…" : "Limpar logs"}</Button>
    </div>
    {loading ? <div role="status" aria-label="Analisando logs internos" className="flex gap-3"><Skeleton className="h-5 w-24" /><Skeleton className="h-5 w-40" /></div> : status && <>
      <p className="text-xs text-muted-foreground"><span className="font-mono text-foreground">{size(status.bytes)}</span> · {status.files} {status.files === 1 ? "arquivo" : "arquivos"}</p>
      <p className="text-[11px] leading-4 text-muted-foreground">Rotação automática: até {size(status.maxBytes)} em {status.maxFiles} arquivos. Os registros mais antigos são substituídos.</p>
      {status.droppedRecordsCurrentRun > 0 && <p className="text-xs text-onedark-yellow">{status.droppedRecordsCurrentRun} registros de diagnóstico não puderam ser gravados nesta execução.</p>}
    </>}
    {error && <div className="flex flex-wrap items-center justify-between gap-2"><p role="alert" className="text-xs text-destructive">{error}</p><Button variant="ghost" size="sm" className="cursor-pointer" disabled={loading || clearing} onClick={() => { void refresh(); }}>Analisar novamente</Button></div>}
    <AlertDialog open={confirm} onOpenChange={open => { if (!clearing) setConfirm(open); }}><ConfirmationDialogContent>
      <AlertDialogHeader><AlertDialogTitle>Limpar logs internos?</AlertDialogTitle><AlertDialogDescription>Serão removidos aproximadamente {size(status?.bytes ?? 0)} em logs de diagnóstico e telemetria deste perfil. Conversas, projetos, configurações e incidentes compartilhados serão preservados. Os logs removidos não poderão ser recuperados.</AlertDialogDescription></AlertDialogHeader>
      <AlertDialogFooter><AlertDialogCancel className="cursor-pointer" disabled={clearing}>Cancelar</AlertDialogCancel><AlertDialogAction variant="destructive" className="cursor-pointer" disabled={clearing} onClick={event => { event.preventDefault(); void clear(); }}>{clearing ? "Limpando…" : "Confirmar limpeza"}</AlertDialogAction></AlertDialogFooter>
    </ConfirmationDialogContent></AlertDialog>
  </section>;
}
