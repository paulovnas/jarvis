import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Terminal, Square, ChevronDown } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel } from "@/components/ui/alert-dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { processSchema, processRunning, PROCESS_LABELS, type ChatProcess } from "@/core/processes";
import { libraryError } from "@/core/library";

function Output({ process }: { process: ChatProcess }) {
  const [result, setResult] = useState<{ output: string; error?: string } | null>(null);
  useEffect(() => {
    let active = true; let timer: ReturnType<typeof setTimeout> | undefined;
    const refresh = async () => {
      try {
        const result = await invoke<{ output: string }>("read_chat_process", { conversationId: process.conversationId, id: process.id });
        if (active) setResult({ output: result.output });
      } catch (cause) { if (active) setResult({ output: "", error: libraryError(cause, "Não foi possível ler a saída.") }); }
      if (active && processRunning(process)) timer = setTimeout(() => void refresh(), 1500);
    };
    void refresh();
    return () => { active = false; if (timer) clearTimeout(timer); };
  }, [process]);
  return !result ? <Skeleton className="mt-2 h-16 w-full" aria-label="Carregando saída" /> : result.error ? <p role="alert" className="mt-2 text-xs text-destructive">{result.error}</p> : <pre aria-label={`Saída de ${process.title}`} className="mt-2 max-h-44 overflow-auto whitespace-pre-wrap break-all rounded-md bg-background p-2 font-mono text-[10px] leading-4 text-muted-foreground">{result.output || "Sem saída."}</pre>;
}

export function ProcessPopover({ conversationId }: { conversationId: string }) {
  const [items, setItems] = useState<ChatProcess[]>([]);
  const [open, setOpen] = useState(false);
  const [logs, setLogs] = useState<string | null>(null);
  const [selected, setSelected] = useState<ChatProcess | null>(null);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lock = useRef(false);
  const refreshRef = useRef<() => Promise<void>>(async () => {});
  useEffect(() => {
    let active = true, running = false, dirty = false;
    const stops: (() => void)[] = [];
    const refresh = async () => {
      if (!active) return;
      if (running) { dirty = true; return; }
      running = true;
      try {
        const result = processSchema.array().parse(await invoke("list_chat_processes", { conversationId }));
        if (active) { setItems(result.filter(item => item.conversationId === conversationId)); setError(null); }
      } catch (cause) { if (active) setError(libraryError(cause, "Não foi possível consultar os processos.")); }
      finally { running = false; if (active && dirty) { dirty = false; void refresh(); } }
    };
    refreshRef.current = refresh;
    void listen<{ conversationId: string }>("processes:changed", event => { if (event.payload.conversationId === conversationId) void refresh(); }).then(stop => { if (active) { stops.push(stop); void refresh(); } else stop(); }).catch(() => { void refresh(); });
    return () => { active = false; stops.forEach(stop => stop()); };
  }, [conversationId]);
  const count = items.filter(processRunning).length;
  async function stop() {
    if (!selected || lock.current) return;
    lock.current = true; setStopping(true);
    try { await invoke("stop_chat_process", { conversationId, id: selected.id, confirmed: true }); setSelected(null); await refreshRef.current(); toast.success("Processo interrompido."); }
    catch (cause) { toast.error(libraryError(cause, "Não foi possível parar o processo.")); }
    finally { lock.current = false; setStopping(false); }
  }
  if (!count && !open && !selected && !error) return null;
  return <>
    <Popover open={open} onOpenChange={value => { setOpen(value); if (value) void refreshRef.current(); else setLogs(null); }}>
      <PopoverTrigger render={<Button variant="ghost" size="sm" />} aria-label={error ? "Consultar processos" : `${count} processos em execução`} className="h-7 cursor-pointer gap-1.5 px-2 font-mono text-[11px] text-onedark-green">
        <Terminal aria-hidden="true" className="size-3.5" /><span>{error ? "!" : count}</span>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" sideOffset={12} className="dark instrument-panel max-h-[65vh] w-[380px] max-w-[90vw] gap-3 overflow-y-auto bg-card p-3 text-foreground">
        <div className="micro-label flex items-center gap-2 text-muted-foreground"><Terminal className="size-3.5" aria-hidden="true" />Processos<Badge variant="secondary" className="ml-auto font-mono text-[10px]">{count}</Badge></div>
        {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
        {items.map(process => <Collapsible key={process.id} open={logs === process.id} onOpenChange={value => setLogs(value ? process.id : null)} className="rounded-md border border-border bg-background/40 p-3 shadow-[inset_0_1px_0_#ffffff08]">
          <div className="flex items-center gap-2"><span className="min-w-0 flex-1 truncate text-xs font-medium">{process.title}</span><Badge variant="outline" className={`text-[9px] ${process.status === "failed" ? "text-destructive" : processRunning(process) ? "text-onedark-green" : "text-muted-foreground"}`}>{PROCESS_LABELS[process.status]}</Badge></div>
          <p className="mt-2 break-all font-mono text-[10px] leading-4 text-muted-foreground">{process.command}</p>
          <p title={process.cwd} className="mt-1 truncate font-mono text-[10px] text-muted-foreground">{process.cwd}</p>
          <div className="mt-2 flex items-center gap-2"><span className="font-mono text-[9px] text-muted-foreground">PID {process.pid} · {new Date(process.startedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}{process.exitCode !== null ? ` · saída ${process.exitCode}` : ""}</span>
            <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="ml-auto h-6 cursor-pointer gap-1 px-1.5 text-[10px]">Saída<ChevronDown className="size-3" /></CollapsibleTrigger>
            {processRunning(process) && <Button variant="ghost" size="sm" disabled={process.status === "stopping"} className="h-6 cursor-pointer gap-1 px-1.5 text-[10px] text-destructive" aria-label={`Parar ${process.title}`} onClick={() => setSelected(process)}><Square className="size-3" />Parar</Button>}
          </div>
          <CollapsibleContent>{open && logs === process.id && <Output process={process} />}</CollapsibleContent>
        </Collapsible>)}
      </PopoverContent>
    </Popover>
    <AlertDialog open={!!selected} onOpenChange={value => { if (!value && !stopping) setSelected(null); }}>
      <AlertDialogContent className="dark"><AlertDialogHeader><AlertDialogTitle>Parar {selected?.title}?</AlertDialogTitle><AlertDialogDescription>O processo e seus subprocessos serão encerrados.</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel disabled={stopping}>Cancelar</AlertDialogCancel><Button variant="destructive" disabled={stopping} onClick={() => void stop()}>Parar processo</Button></AlertDialogFooter></AlertDialogContent>
    </AlertDialog>
  </>;
}
