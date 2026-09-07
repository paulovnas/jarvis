import { useState, type CSSProperties } from "react";
import { Gauge, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";
import { conversationContext } from "@/core/inspector";
import type { ContextInfo } from "@/core/chat";

const format = (value: number) => value.toLocaleString("pt-BR");

function contextColor(percent: number | null) {
  if (percent === null) return "var(--muted-foreground)";
  const value = Math.max(0, Math.min(100, percent));
  const green = [152, 195, 121], yellow = [229, 192, 123], red = [224, 108, 117];
  const [from, to, ratio] = value <= 60 ? [green, yellow, value / 60] as const : [yellow, red, (value - 60) / 40] as const;
  return `rgb(${from.map((channel, index) => Math.round(channel + (to[index] - channel) * ratio)).join(", ")})`;
}

export function ContextUsage({ context, live, onCompact, compacting = false, disabled = false }: {
  context: ReturnType<typeof conversationContext>; live?: ContextInfo;
  onCompact?: () => Promise<boolean>; compacting?: boolean; disabled?: boolean;
}) {
  const [confirming, setConfirming] = useState(false);
  const [pending, setPending] = useState(false);
  const tokens = live?.tokens ?? context.tokens;
  const limit = live ? live.limit : context.limit;
  const percent = limit && tokens !== null ? tokens / limit * 100 : null;
  const estimated = live?.estimated ?? context.estimatedTokens > 0;
  const busy = pending || compacting || live?.compacting === true;
  const color = contextColor(percent);
  return <footer aria-label="Contexto da conversa" aria-busy={busy} data-compacting={busy || undefined} className="context-meter shrink-0 border-t border-border bg-sidebar px-4 py-3" style={{ "--context-color": color } as CSSProperties}>
    <div className="flex items-center gap-2 text-xs">
      <Gauge aria-hidden="true" className="size-3.5 transition-colors motion-reduce:transition-none" style={{ color }} /><span className="micro-label">Contexto</span>
      <span className="ml-auto font-mono text-[11px] tabular-nums transition-colors motion-reduce:transition-none" style={{ color }}>{percent === null ? "—" : `${Math.round(percent)}%`}</span>
    </div>
    <div className="mt-2 flex items-center gap-2">
      <Button variant="ghost" size="icon" aria-label="Compactar contexto" title="Compactar contexto" disabled={disabled || busy || !onCompact} className="size-6 shrink-0 cursor-pointer text-muted-foreground" onClick={() => setConfirming(true)}><RefreshCw aria-hidden="true" className={`size-3.5 ${busy ? "animate-spin motion-reduce:animate-none" : ""}`} /></Button>
      {percent !== null ? <Progress aria-label="Ocupação da janela de contexto" value={Math.min(100, percent)} className="min-w-0 flex-1 [&_[data-slot=progress-indicator]]:bg-[var(--context-color)] [&_[data-slot=progress-indicator]]:motion-reduce:transition-none" /> : <div className="h-1 flex-1 rounded-full bg-muted" />}
    </div>
    {tokens !== null && <p className="mt-1 text-right font-mono text-[10px] text-muted-foreground tabular-nums">{estimated ? "≈ " : ""}{format(tokens)}{limit ? ` / ${format(limit)}` : ""} tokens</p>}
    <AlertDialog open={confirming} onOpenChange={setConfirming}>
      <AlertDialogContent><AlertDialogHeader><AlertDialogTitle>Compactar contexto?</AlertDialogTitle><AlertDialogDescription>O histórico completo será preservado.</AlertDialogDescription></AlertDialogHeader>
        <AlertDialogFooter><AlertDialogCancel className="cursor-pointer">Cancelar</AlertDialogCancel><AlertDialogAction className="cursor-pointer" disabled={disabled || busy || !onCompact} onClick={() => { setConfirming(false); setPending(true); void onCompact?.().finally(() => setPending(false)); }}>Compactar</AlertDialogAction></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </footer>;
}
