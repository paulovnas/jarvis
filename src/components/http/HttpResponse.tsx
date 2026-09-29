import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Copy, Download, Sparkles } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Skeleton } from "@/components/ui/skeleton";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Separator } from "@/components/ui/separator";
import { httpResultSchema, type HttpResult, type HttpRun } from "@/core/http-client";
import { libraryError } from "@/core/library";
import { writeClipboardText } from "@/core/clipboard";

import { httpRunLabels, httpSize } from "@/core/http-presentation";

export function HttpResponse({ conversationId, run, onAnalyze }: { conversationId: string; run: HttpRun | null; onAnalyze: (run: HttpRun) => void }) {
  const [result, setResult] = useState<HttpResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState(false);
  const runId = run?.id;
  const runStatus = run?.status;
  useEffect(() => {
    if (!runId || runStatus === "running") return;
    let alive = true;
    const read = async () => {
      setLoading(true); setError(null);
      try {
        const value = httpResultSchema.parse(await invoke("get_http_result", { conversationId, runId, offset, limit: 64 * 1024 }));
        if (alive) setResult(value);
      } catch (cause) { if (alive) setError(libraryError(cause, "Não foi possível ler esta resposta.")); } finally { if (alive) setLoading(false); }
    };
    void read();
    return () => { alive = false; };
  }, [conversationId, offset, runId, runStatus]);
  if (!run) return <Empty className="min-h-40 border border-dashed"><EmptyHeader><EmptyTitle>Nenhuma resposta selecionada</EmptyTitle><EmptyDescription>Envie a requisição ou abra uma execução do histórico.</EmptyDescription></EmptyHeader></Empty>;
  const current = result?.run.id === run.id && result.offset === offset ? result : null;
  let text = current?.text ?? run.preview;
  if (current && !current.binary && current.offset === 0 && current.nextOffset === null) {
    try { text = JSON.stringify(JSON.parse(text), null, 2); } catch { /* Keep non-JSON responses inert. */ }
  }
  const copy = async () => { try { await writeClipboardText(text); toast.success("Trecho da resposta copiado"); } catch { toast.error("Não foi possível copiar a resposta."); } };
  const download = async () => {
    setExporting(true);
    try {
      const extension = run.mime.includes("json") ? "json" : run.mime.startsWith("text/") ? "txt" : "bin";
      const path = await save({ defaultPath: `resposta-${run.id}.${extension}`, title: "Salvar resposta HTTP original" });
      if (path) { await invoke("save_http_response", { conversationId, runId: run.id, path }); toast.success("Resposta original salva"); }
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível salvar a resposta.")); } finally { setExporting(false); }
  };
  return <section aria-label="Resposta HTTP" className="flex min-w-0 flex-col gap-3">
    <div className="flex flex-wrap items-center gap-2"><Badge variant="outline" className={run.httpStatus && run.httpStatus >= 400 ? "text-onedark-yellow" : run.httpStatus ? "text-onedark-green" : "text-muted-foreground"}>{run.httpStatus ?? httpRunLabels[run.status]}</Badge><span className="font-mono text-xs text-muted-foreground">{run.elapsedMs.toLocaleString("pt-BR")} ms · {httpSize(run.receivedBytes)} recebidos · {httpSize(run.storedBytes)} armazenados</span><span className="flex-1" /><Button size="sm" variant="secondary" disabled={run.status === "running"} className="cursor-pointer" onClick={() => onAnalyze(run)}><Sparkles />Analisar com IA</Button><Button size="sm" variant="outline" disabled={run.status === "running" || !text || current?.binary} className="cursor-pointer" onClick={() => void copy()}><Copy />Copiar trecho</Button><Button size="sm" variant="outline" disabled={exporting || run.status === "running" || run.bodyExpired} className="cursor-pointer" onClick={() => void download()}><Download />Salvar original</Button></div>
    <p className="break-all font-mono text-[11px] text-muted-foreground">{run.request.method} {run.url || run.request.url} · execução {run.id}</p>
    {run.outcomeUncertain && <Alert><AlertDescription>A resposta não foi confirmada. O servidor pode ter aplicado a operação; verifique o resultado antes de reenviar.</AlertDescription></Alert>}
    {run.error && <Alert variant="destructive"><AlertDescription>{run.error}</AlertDescription></Alert>}
    {(run.truncated || run.bodyExpired) && <Alert><AlertDescription>{run.bodyExpired ? "O corpo desta resposta expirou. Os metadados da execução foram preservados." : "O corpo atingiu o limite de armazenamento. O conteúdo disponível está incompleto."}</AlertDescription></Alert>}
    <Tabs defaultValue="body" className="min-w-0 gap-3"><TabsList className="h-8 w-fit"><TabsTrigger value="body" className="cursor-pointer text-xs">Resposta</TabsTrigger><TabsTrigger value="headers" className="cursor-pointer text-xs">Headers ({run.headers.length})</TabsTrigger><TabsTrigger value="details" className="cursor-pointer text-xs">Execução</TabsTrigger></TabsList>
      <TabsContent value="body" className="min-w-0">{run.status === "running" ? <div role="status" aria-label="Aguardando resposta HTTP" className="flex flex-col gap-2"><Skeleton className="h-4 w-2/3" /><Skeleton className="h-4 w-1/2" /></div> : loading ? <div role="status" aria-label="Carregando corpo da resposta" className="flex flex-col gap-2"><Skeleton className="h-4 w-3/4" /><Skeleton className="h-4 w-1/2" /></div> : error ? <p role="alert" className="text-sm text-destructive">{error}</p> : current?.binary ? <Empty className="border"><EmptyHeader><EmptyTitle>Resposta binária</EmptyTitle><EmptyDescription>{run.mime || "Tipo de conteúdo não informado"}. Use Salvar original para baixar os bytes recebidos.</EmptyDescription></EmptyHeader></Empty> : <pre aria-label="Corpo da resposta" className="max-h-[50vh] min-h-24 overflow-auto whitespace-pre-wrap break-all rounded-md border border-border bg-sidebar p-3 font-mono text-xs">{text || (run.bodyExpired ? "Corpo indisponível." : "Resposta sem corpo.")}</pre>}
        {current && (offset > 0 || current.nextOffset !== null) && <div className="mt-2 flex items-center justify-end gap-2"><span className="font-mono text-[10px] text-muted-foreground">A partir do byte {offset.toLocaleString("pt-BR")}</span><Button size="sm" variant="ghost" disabled={loading || offset === 0} className="cursor-pointer" onClick={() => setOffset(Math.max(0, offset - 64 * 1024))}>Anterior</Button><Button size="sm" variant="ghost" disabled={loading || current.nextOffset === null} className="cursor-pointer" onClick={() => { if (current.nextOffset !== null) setOffset(current.nextOffset); }}>Próximo trecho</Button></div>}
      </TabsContent>
      <TabsContent value="headers"><div className="flex flex-col gap-2">{run.headers.map((header, index) => <div key={index} className="grid min-w-0 grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-3 border-b border-border pb-2 font-mono text-xs"><span className="break-all text-muted-foreground">{header.name}</span><span className="break-all">{header.value}</span></div>)}{!run.headers.length && <p className="text-xs text-muted-foreground">Nenhum header recebido.</p>}</div></TabsContent>
      <TabsContent value="details"><div className="flex flex-col gap-2 text-xs text-muted-foreground"><p>{httpRunLabels[run.status]} · início {new Date(run.startedAt).toLocaleString("pt-BR")}</p><p>Requisição: {run.request.name} · revisão {run.draftRevision} · ambiente {run.environmentId ?? "compartilhado"}, revisão {run.environmentRevision}</p><p className="font-mono">{run.mime || "Sem Content-Type"}</p>{run.redirects.length > 0 && <><Separator /><p>Redirecionamentos</p>{run.redirects.map((url, index) => <p key={index} className="break-all font-mono">{index + 1}. {url}</p>)}</>}</div></TabsContent>
    </Tabs>
  </section>;
}
