import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { Archive, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { libraryError } from "@/core/library";

const previewSchema = z.object({ days: z.number(), bytes: z.number().nonnegative(), protected: z.number().int().nonnegative(), conversations: z.array(z.object({ id: z.string(), title: z.string(), projectName: z.string(), projectId: z.string(), activity: z.number(), bytes: z.number().nonnegative() })) });
const resultSchema = z.object({ deleted: z.number().int().nonnegative(), skipped: z.number().int().nonnegative(), failed: z.number().int().nonnegative(), bytes: z.number().nonnegative() });
function size(bytes: number) { return bytes < 1024 * 1024 ? `${(bytes / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} KB` : `${(bytes / 1024 / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} MB`; }

export function ChatCleanupSettings() {
  const [days, setDays] = useState("7");
  const [preview, setPreview] = useState<z.infer<typeof previewSchema> | null>(null);
  const [loading, setLoading] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lock = useRef(false);
  const analyze = async () => {
    if (lock.current) return;
    lock.current = true; setLoading(true); setError(null);
    try { setPreview(previewSchema.parse(await invoke("preview_chat_cleanup", { days: Number(days) }))); }
    catch (cause) { setError(libraryError(cause, "Não foi possível analisar as conversas.")); }
    finally { lock.current = false; setLoading(false); }
  };
  const clean = async () => {
    if (!preview || !preview.conversations.length || lock.current) return;
    lock.current = true; setDeleting(true); setError(null);
    try {
      const result = resultSchema.parse(await invoke("cleanup_old_chats", { days: preview.days, selection: preview.conversations.map(({ id, activity }) => ({ id, activity })), confirmed: true }));
      if (result.failed) toast.error(`${result.deleted} conversas excluídas. ${result.failed} exclusões precisam ser verificadas; analise novamente.`);
      else if (result.deleted) toast.success(`${result.deleted} ${result.deleted === 1 ? "conversa excluída" : "conversas excluídas"} · ${size(result.bytes)} liberados`);
      else toast.info("Nenhuma conversa excluída");
      if (result.skipped) toast.info(`${result.skipped} ${result.skipped === 1 ? "conversa preservada" : "conversas preservadas"} após nova verificação`);
      setPreview(null);
    } catch (cause) { setError(libraryError(cause, "Não foi possível concluir a limpeza. Analise novamente antes de tentar.")); setPreview(null); }
    finally { lock.current = false; setDeleting(false); }
  };
  return <section aria-labelledby="cleanup-title" className="mt-7 space-y-3 border-t border-border pt-6">
    <h2 id="cleanup-title" className="micro-label flex items-center gap-2 text-muted-foreground"><Archive className="size-3.5" />Limpeza</h2>
    <Card className="gap-3 p-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="space-y-2"><Label htmlFor="cleanup-period" className="text-xs">Conversas sem atividade há mais de</Label>
          <Select value={days} onValueChange={value => { if (value) { setDays(value); setPreview(null); } }} disabled={loading || deleting}>
            <SelectTrigger id="cleanup-period" className="w-36 cursor-pointer"><SelectValue>{days} dias</SelectValue></SelectTrigger>
            <SelectContent>{[7, 14, 30, 90].map(value => <SelectItem key={value} value={String(value)} className="cursor-pointer">{value} dias</SelectItem>)}</SelectContent>
          </Select>
        </div>
        <Button variant="outline" size="sm" disabled={loading || deleting} onClick={() => { void analyze(); }} className="cursor-pointer gap-2"><Trash2 className="size-3.5" />Revisar limpeza</Button>
      </div>
      <p className="text-xs text-muted-foreground">A conversa mais recente de cada projeto será mantida.</p>
      {loading && <div role="status" aria-label="Analisando históricos" className="flex gap-3"><Skeleton className="h-4 w-1/2" /><Skeleton className="h-4 w-20" /></div>}
      {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
      {preview?.conversations.length === 0 && <p role="status" className="text-xs text-muted-foreground">Nenhuma conversa disponível para limpeza.{preview.protected > 0 && ` ${preview.protected} protegidas ou indisponíveis.`}</p>}
    </Card>
    <AlertDialog open={Boolean(preview?.conversations.length)} onOpenChange={open => { if (!open && !deleting) setPreview(null); }}>
      <AlertDialogContent className="dark flex max-h-[80dvh] flex-col gap-4 border-border bg-card sm:max-w-lg">
        <AlertDialogHeader>
          <AlertDialogTitle>Excluir {preview?.conversations.length} conversas?</AlertDialogTitle>
          <AlertDialogDescription className="text-xs">Exclusão definitiva dos históricos. As pastas dos projetos serão preservadas, assim como a conversa mais recente de cada projeto.</AlertDialogDescription>
        </AlertDialogHeader>
        <div className="flex justify-between font-mono text-xs text-muted-foreground"><span>Sem atividade há {preview?.days} dias</span><span>≈ {size(preview?.bytes ?? 0)}</span></div>
        <ul aria-label="Conversas para excluir" className="min-h-0 max-h-64 divide-y divide-border overflow-y-auto rounded-md border border-border px-3">
          {preview?.conversations.map(item => <li key={item.id} className="flex items-center justify-between gap-4 py-2.5"><div className="min-w-0"><p className="truncate text-xs font-medium">{item.title}</p><p className="truncate text-[10px] text-muted-foreground">{item.projectName}</p></div><span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">{new Date(item.activity * 1000).toLocaleDateString("pt-BR")}</span></li>)}
        </ul>
        {(preview?.protected ?? 0) > 0 && <p className="text-xs text-muted-foreground">{preview?.protected} conversas protegidas ou indisponíveis.</p>}
        {preview?.conversations.length === 500 && <p className="text-xs text-muted-foreground">Até 500 conversas por limpeza.</p>}
        <AlertDialogFooter><AlertDialogCancel disabled={deleting} className="cursor-pointer">Cancelar</AlertDialogCancel><AlertDialogAction variant="destructive" disabled={deleting} className="cursor-pointer" onClick={event => { event.preventDefault(); void clean(); }}>{deleting ? "Excluindo…" : "Excluir conversas"}</AlertDialogAction></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </section>;
}
