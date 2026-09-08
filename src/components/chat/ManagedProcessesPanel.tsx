import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, Square, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { ConfirmationDialogContent } from "@/components/ConfirmationDialogContent";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Skeleton } from "@/components/ui/skeleton";
import { libraryError } from "@/core/library";
import { PROCESS_LABELS, processRunning, type ChatProcess } from "@/core/processes";

function ProcessOutput({ conversationId, process }: { conversationId: string; process: ChatProcess }) {
  const [result, setResult] = useState<{ output: string; error?: string }>();
  const running = processRunning(process);

  useEffect(() => {
    let current = true;
    const receive = (value: { output: string }) => {
      if (current) setResult({ output: value.output });
    };
    const fail = (error: unknown) => {
      if (current) setResult({ output: "", error: libraryError(error, "Não foi possível ler a saída.") });
    };
    void invoke<{ output: string }>("read_chat_process", { conversationId, id: process.id }).then(receive, fail);
    const timer = running ? window.setInterval(() => {
      void invoke<{ output: string }>("read_chat_process", { conversationId, id: process.id }).then(receive, fail);
    }, 1_500) : undefined;
    return () => {
      current = false;
      if (timer !== undefined) clearInterval(timer);
    };
  }, [conversationId, process.id, running]);

  if (!result) return <Skeleton className="mt-2 h-16 w-full" aria-label="Carregando saída" />;
  if (result.error) return <p role="alert" className="mt-2 text-xs text-destructive">{result.error}</p>;
  return <pre aria-label={`Saída de ${process.title}`} className="mt-2 max-h-44 overflow-auto whitespace-pre-wrap break-all rounded-md bg-sidebar p-2 font-mono text-[10px] leading-4 text-muted-foreground">{result.output || "Sem saída."}</pre>;
}

export function ManagedProcessesPanel({
  conversationId,
  processes,
  onRemove,
  onStop,
}: {
  conversationId: string;
  processes: ChatProcess[];
  onRemove: (id: string) => void;
  onStop: (id: string) => void;
}) {
  const [logs, setLogs] = useState<string | null>(null);
  const [selected, setSelected] = useState<ChatProcess | null>(null);
  const [stopping, setStopping] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const lock = useRef(false);

  const remove = async (process: ChatProcess) => {
    if (processRunning(process) || lock.current) return;
    lock.current = true;
    setRemoving(process.id);
    try {
      await invoke("remove_chat_process", { conversationId, id: process.id });
      setLogs(value => value === process.id ? null : value);
      onRemove(process.id);
    } catch (error) {
      toast.error(libraryError(error, "Não foi possível remover o processo."));
    } finally {
      lock.current = false;
      setRemoving(null);
    }
  };

  const stop = async () => {
    if (!selected || lock.current) return;
    lock.current = true;
    setStopping(true);
    try {
      await invoke("stop_chat_process", { conversationId, id: selected.id, confirmed: true });
      onStop(selected.id);
      setSelected(null);
      toast.success("Processo interrompido.");
    } catch (error) {
      toast.error(libraryError(error, "Não foi possível parar o processo."));
    } finally {
      lock.current = false;
      setStopping(false);
    }
  };

  if (processes.length === 0) {
    return <div className="flex h-full items-center justify-center rounded-lg border border-dashed border-border bg-sidebar/50 text-center"><p className="text-sm text-muted-foreground">Nenhum processo gerenciado.</p></div>;
  }

  return <>
    <div className="space-y-2">
      {processes.map(process => <Collapsible key={process.id} open={logs === process.id} onOpenChange={next => setLogs(next ? process.id : null)} className="rounded-md border border-border bg-sidebar/50 p-3 shadow-[inset_0_1px_0_var(--border)]">
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-xs font-medium">{process.title}</span>
          <Badge variant="outline" className={`text-[9px] ${process.status === "failed" ? "text-destructive" : processRunning(process) ? "text-onedark-green" : "text-muted-foreground"}`}>{PROCESS_LABELS[process.status]}</Badge>
        </div>
        <p className="mt-2 break-all font-mono text-[10px] leading-4 text-muted-foreground">{process.command}</p>
        <p title={process.cwd} className="mt-1 truncate font-mono text-[10px] text-muted-foreground">{process.cwd}</p>
        <div className="mt-2 flex items-center gap-2">
          <span className="font-mono text-[9px] text-muted-foreground">PID {process.pid} · {new Date(process.startedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}{process.exitCode !== null ? ` · saída ${process.exitCode}` : ""}</span>
          <CollapsibleTrigger render={<Button type="button" variant="ghost" size="sm" />} className="ml-auto h-6 cursor-pointer gap-1 px-1.5 text-[10px]">Saída<ChevronDown className="size-3" /></CollapsibleTrigger>
          {processRunning(process) ? <Button type="button" variant="ghost" size="sm" disabled={process.status === "stopping"} className="h-6 cursor-pointer gap-1 px-1.5 text-[10px] text-destructive" aria-label={`Parar ${process.title}`} onClick={() => setSelected(process)}><Square className="size-3" />Parar</Button> : <Button type="button" variant="ghost" size="sm" disabled={removing !== null || stopping} className="h-6 cursor-pointer gap-1 px-1.5 text-[10px] text-muted-foreground" aria-label={`Remover ${process.title}`} onClick={() => { void remove(process); }}><Trash2 className="size-3" />Remover</Button>}
        </div>
        <CollapsibleContent>{logs === process.id && <ProcessOutput conversationId={conversationId} process={process} />}</CollapsibleContent>
      </Collapsible>)}
    </div>
    <AlertDialog open={selected !== null} onOpenChange={next => { if (!next && !stopping) setSelected(null); }}>
      <ConfirmationDialogContent aria-describedby={undefined}>
        <AlertDialogHeader><AlertDialogTitle>Parar {selected?.title}?</AlertDialogTitle><AlertDialogDescription>O processo e seus subprocessos serão encerrados.</AlertDialogDescription></AlertDialogHeader>
        <AlertDialogFooter><AlertDialogCancel className="cursor-pointer" disabled={stopping}>Cancelar</AlertDialogCancel><Button type="button" data-confirm-action variant="destructive" className="cursor-pointer" disabled={stopping} onClick={() => { void stop(); }}>Parar processo</Button></AlertDialogFooter>
      </ConfirmationDialogContent>
    </AlertDialog>
  </>;
}
