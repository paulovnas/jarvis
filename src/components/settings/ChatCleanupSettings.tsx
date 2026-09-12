import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { Archive, Database, HardDrive, Sparkles, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Progress } from "@/components/ui/progress";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";
import { libraryError } from "@/core/library";
import { getJournalMaintenanceStatus, optimizeJournals, type JournalMaintenanceProgress, type JournalMaintenanceSummary } from "@/core/journal-maintenance";
import { skillCacheCleanupSchema, skillCacheStatusSchema, type SkillCacheStatus } from "@/core/skills";

const previewSchema = z.object({ days: z.number(), bytes: z.number().nonnegative(), protected: z.number().int().nonnegative(), conversations: z.array(z.object({ id: z.string(), title: z.string(), projectName: z.string(), projectId: z.string(), activity: z.number(), bytes: z.number().nonnegative() })) });
const resultSchema = z.object({ deleted: z.number().int().nonnegative(), skipped: z.number().int().nonnegative(), failed: z.number().int().nonnegative(), bytes: z.number().nonnegative() });
function size(bytes: number) {
  if (bytes < 1024) return `${bytes.toLocaleString("pt-BR")} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 2 })} GB`;
}
async function readSkillCache() {
  return skillCacheStatusSchema.parse(await invoke("get_skill_cache_status"));
}

export function ChatCleanupSettings() {
  const [days, setDays] = useState("7");
  const [preview, setPreview] = useState<z.infer<typeof previewSchema> | null>(null);
  const [loading, setLoading] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cache, setCache] = useState<SkillCacheStatus | null>(null);
  const [cacheLoading, setCacheLoading] = useState(true);
  const [cacheClearing, setCacheClearing] = useState(false);
  const [cacheError, setCacheError] = useState<string | null>(null);
  const [confirmCache, setConfirmCache] = useState(false);
  const [journals, setJournals] = useState<JournalMaintenanceSummary | null>(null);
  const [journalsLoading, setJournalsLoading] = useState(true);
  const [journalsOptimizing, setJournalsOptimizing] = useState(false);
  const [journalsProgress, setJournalsProgress] = useState<JournalMaintenanceProgress | null>(null);
  const [journalsError, setJournalsError] = useState<string | null>(null);
  const lock = useRef(false);
  const cacheLock = useRef(false);
  const journalLock = useRef(false);
  const refreshCache = useCallback(async () => {
    if (cacheLock.current) return;
    cacheLock.current = true; setCacheLoading(true); setCacheError(null);
    try { setCache(await readSkillCache()); }
    catch (cause) { setCacheError(libraryError(cause, "Não foi possível analisar o cache de recursos.")); }
    finally { cacheLock.current = false; setCacheLoading(false); }
  }, []);
  const refreshJournals = useCallback(async () => {
    if (journalLock.current) return;
    journalLock.current = true; setJournalsLoading(true); setJournalsError(null);
    try { setJournals(await getJournalMaintenanceStatus()); }
    catch (cause) { setJournalsError(libraryError(cause, "Não foi possível analisar os históricos.")); }
    finally { journalLock.current = false; setJournalsLoading(false); }
  }, []);
  useEffect(() => {
    let active = true;
    void readSkillCache()
      .then(result => { if (active) setCache(result); })
      .catch(cause => { if (active) setCacheError(libraryError(cause, "Não foi possível analisar o cache de recursos.")); })
      .finally(() => { if (active) setCacheLoading(false); });
    return () => { active = false; };
  }, []);
  useEffect(() => {
    let active = true;
    void getJournalMaintenanceStatus()
      .then(result => { if (active) setJournals(result); })
      .catch(cause => { if (active) setJournalsError(libraryError(cause, "Não foi possível analisar os históricos.")); })
      .finally(() => { if (active) setJournalsLoading(false); });
    return () => { active = false; };
  }, []);
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
  const cleanCache = async () => {
    if (cacheLock.current) return;
    cacheLock.current = true; setCacheClearing(true); setCacheError(null);
    try {
      const result = skillCacheCleanupSchema.parse(await invoke("clear_skill_cache", { confirmed: true }));
      setCache(result.status);
      toast.success(result.freedBytes ? `${size(result.freedBytes)} liberados do cache de recursos` : "O cache de recursos já estava vazio");
    } catch (cause) { setCacheError(libraryError(cause, "Não foi possível limpar o cache de recursos.")); }
    finally { cacheLock.current = false; setCacheClearing(false); setConfirmCache(false); }
  };
  const optimize = async () => {
    if (journalLock.current || !journals?.candidates) return;
    journalLock.current = true; setJournalsOptimizing(true); setJournalsError(null); setJournalsProgress(null);
    try {
      const result = await optimizeJournals(setJournalsProgress);
      setJournals(result.status);
      if (result.failedFiles) toast.error(`${result.optimizedFiles} históricos otimizados. ${result.failedFiles} arquivos foram preservados após falha.`);
      else toast.success(result.recoveredBytes ? `${size(result.recoveredBytes)} liberados dos históricos` : "Os históricos já estavam compactos");
    } catch (cause) { setJournalsError(libraryError(cause, "Não foi possível otimizar os históricos.")); }
    finally { journalLock.current = false; setJournalsOptimizing(false); setJournalsProgress(null); }
  };
  const journalPercent = journalsProgress?.totalFiles
    ? journalsProgress.processedFiles / journalsProgress.totalFiles * 100
    : null;
  return <section aria-labelledby="cleanup-title" className="mt-8 space-y-4 border-t border-border pt-7">
    <h2 id="cleanup-title" className="micro-label flex items-center gap-2 text-muted-foreground"><Archive className="size-3.5" />Limpeza</h2>
    <div className="grid items-start gap-4 lg:grid-cols-2">
    <Card className="min-w-0 gap-4 p-5">
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
    <Card role="region" aria-label="Cache de recursos" className="min-w-0 gap-4 p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3"><div className="flex size-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary"><Database className="size-4 text-onedark-yellow" /></div><div className="min-w-0"><p className="text-xs font-medium">Cache do Marketplace</p><p className="text-[11px] text-muted-foreground">Repositórios reutilizados nas consultas e atualizações de skills.</p></div></div>
        <Button variant="outline" size="sm" disabled={cacheLoading || cacheClearing || !cache?.bytes} onClick={() => setConfirmCache(true)} className="cursor-pointer gap-2"><Trash2 className="size-3.5" />Limpar cache</Button>
      </div>
      {cacheLoading && <div role="status" aria-label="Analisando cache de recursos" className="flex gap-3"><Skeleton className="h-4 w-28" /><Skeleton className="h-4 w-48" /></div>}
      {!cacheLoading && cache && <div className="flex flex-wrap items-baseline justify-between gap-2 rounded-md border border-border bg-secondary/50 px-3 py-2"><span className="font-mono text-sm text-foreground">{size(cache.bytes)}</span><span className="text-[11px] text-muted-foreground">{cache.repositories} {cache.repositories === 1 ? "repositório" : "repositórios"}{cache.residues > 0 && ` · ${cache.residues} ${cache.residues === 1 ? "resíduo antigo" : "resíduos antigos"}`}</span></div>}
      {cacheError && <div className="flex flex-wrap items-center justify-between gap-2"><p role="alert" className="text-xs text-destructive">{cacheError}</p><Button variant="ghost" size="sm" disabled={cacheLoading || cacheClearing} onClick={() => { void refreshCache(); }} className="cursor-pointer">Tentar novamente</Button></div>}
      <p className="text-xs text-muted-foreground">Skills instaladas e suas configurações são preservadas. Resíduos de downloads interrompidos também são removidos automaticamente ao iniciar o Jarvis.</p>
    </Card>
    <Card role="region" aria-label="Otimização dos históricos" className="min-w-0 gap-4 p-5 lg:col-span-2">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3"><div className="flex size-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary"><HardDrive className="size-4 text-onedark-cyan" /></div><div className="min-w-0"><p className="text-xs font-medium">Históricos de conversas e agentes</p><p className="text-[11px] text-muted-foreground">Compacta revisões antigas sem alterar o estado atual das conversas.</p></div></div>
        <Button variant="outline" size="sm" disabled={journalsLoading || journalsOptimizing || !journals?.candidates} onClick={() => { void optimize(); }} className="cursor-pointer gap-2"><Sparkles className="size-3.5" />{journalsOptimizing ? "Otimizando…" : "Otimizar históricos"}</Button>
      </div>
      {journalsLoading && <div role="status" aria-label="Analisando arquivos de histórico" className="grid gap-2 sm:grid-cols-3"><Skeleton className="h-12" /><Skeleton className="h-12" /><Skeleton className="h-12" /></div>}
      {!journalsLoading && journals && <div className="grid gap-2 sm:grid-cols-3">
        <div className="rounded-md border border-border bg-secondary/50 px-3 py-2"><p className="font-mono text-sm text-foreground">{size(journals.currentBytes)}</p><p className="text-[10px] text-muted-foreground">{journals.files} {journals.files === 1 ? "arquivo de histórico analisado" : "arquivos de histórico analisados"}</p></div>
        <div className="rounded-md border border-border bg-secondary/50 px-3 py-2"><p className="font-mono text-sm text-onedark-green">{size(journals.recoverableBytes)}</p><p className="text-[10px] text-muted-foreground">espaço recuperável em {journals.candidates} {journals.candidates === 1 ? "arquivo" : "arquivos"}</p></div>
        <div className="rounded-md border border-border bg-secondary/50 px-3 py-2"><p className="font-mono text-sm text-foreground">{(journals.maxAmplificationBps / 100).toLocaleString("pt-BR", { maximumFractionDigits: 1 })}×</p><p className="text-[10px] text-muted-foreground">maior amplificação · {journals.obsoleteRecords.toLocaleString("pt-BR")} revisões obsoletas</p></div>
      </div>}
      {journalsProgress && <div role="status" aria-label="Progresso da otimização dos históricos" className="space-y-2">
        <div className="flex items-center justify-between gap-3 text-[11px] text-muted-foreground"><span>{journalsProgress.phase === "analyzing" ? "Analisando históricos" : journalsProgress.phase === "compacting" ? "Compactando históricos" : "Otimização concluída"}</span><span className="font-mono tabular-nums">{journalsProgress.processedFiles}/{journalsProgress.totalFiles}{journalsProgress.recoveredBytes > 0 && ` · ${size(journalsProgress.recoveredBytes)}`}</span></div>
        <Progress value={journalPercent} aria-label="Arquivos processados" className={journalPercent === null ? "core-install-progress" : undefined} />
      </div>}
      {journals && journals.protectedFiles > 0 && <p className="text-xs text-muted-foreground">{journals.protectedFiles} {journals.protectedFiles === 1 ? "histórico em uso foi preservado" : "históricos em uso foram preservados"}; eles entram na próxima análise.</p>}
      {journals && journals.invalidFiles > 0 && <p className="text-xs text-onedark-yellow">{journals.invalidFiles} {journals.invalidFiles === 1 ? "arquivo inválido foi mantido intacto" : "arquivos inválidos foram mantidos intactos"}.</p>}
      {journalsError && <div className="flex flex-wrap items-center justify-between gap-2"><p role="alert" className="text-xs text-destructive">{journalsError}</p><Button variant="ghost" size="sm" disabled={journalsLoading || journalsOptimizing} onClick={() => { void refreshJournals(); }} className="cursor-pointer">Tentar novamente</Button></div>}
      <p className="text-xs text-muted-foreground">A troca só acontece depois que o novo arquivo é sincronizado, relido e comparado ao estado original. Conversas e subagentes em execução nunca são modificados.</p>
    </Card>
    </div>
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
    <AlertDialog open={confirmCache} onOpenChange={open => { if (!cacheClearing) setConfirmCache(open); }}>
      <AlertDialogContent className="dark border-border bg-card sm:max-w-md">
        <AlertDialogHeader><AlertDialogTitle>Limpar o cache do Marketplace?</AlertDialogTitle><AlertDialogDescription>Serão removidos aproximadamente {size(cache?.bytes ?? 0)} em repositórios temporários. As skills já instaladas e suas configurações serão preservadas.</AlertDialogDescription></AlertDialogHeader>
        <AlertDialogFooter><AlertDialogCancel disabled={cacheClearing} className="cursor-pointer">Cancelar</AlertDialogCancel><AlertDialogAction variant="destructive" disabled={cacheClearing} className="cursor-pointer" onClick={event => { event.preventDefault(); void cleanCache(); }}>{cacheClearing ? "Limpando…" : "Limpar cache"}</AlertDialogAction></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </section>;
}
