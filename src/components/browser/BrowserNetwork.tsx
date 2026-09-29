import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/TextInput";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { browserNetworkSchema } from "@/core/browser";
import type { BrowserController } from "@/hooks/use-browser";
import type { z } from "zod";

export function BrowserNetwork({ browser, id }: { browser: BrowserController; id: string }) {
  const [data, setData] = useState<z.infer<typeof browserNetworkSchema> | null>(null);
  const [filter, setFilter] = useState("");
  const [query, setQuery] = useState({ filter: "", offset: 0, revision: 0 });
  const [busy, setBusy] = useState(false);
  const command = browser.command;
  useEffect(() => {
    let alive = true;
    const read = async () => {
      setBusy(true);
      try {
        const value = await command({ action: "network", id, filter: query.filter, offset: query.offset, limit: 25 });
        if (!alive || value === undefined) return;
        const parsed = browserNetworkSchema.safeParse(value);
        if (parsed.success) setData(parsed.data);
        else toast.error("Não foi possível ler as requisições da página.");
      } finally { if (alive) setBusy(false); }
    };
    void read();
    return () => { alive = false; };
  }, [command, id, query]);
  return <section aria-label="Rede da página" className="flex h-56 min-h-0 shrink-0 flex-col border-t border-border bg-sidebar">
    <form className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border p-2" onSubmit={event => { event.preventDefault(); setQuery(current => ({ filter, offset: 0, revision: current.revision + 1 })); }}><span className="text-xs font-medium">Rede</span><Badge variant="secondary" className="font-mono text-[10px]">{data?.total ?? 0}</Badge><Input aria-label="Filtrar requisições" value={filter} onChange={event => setFilter(event.target.value)} placeholder="Filtrar URL" className="h-7 min-w-20 flex-1 text-xs" /><Button type="submit" size="sm" variant="ghost" disabled={busy} className="h-7 cursor-pointer text-xs">Atualizar rede</Button></form>
    <div className="min-h-0 flex-1 overflow-auto px-3 py-2 font-mono text-[11px]">{busy && !data ? <div role="status" aria-label="Carregando requisições" className="space-y-2"><Skeleton className="h-3" /><Skeleton className="h-3 w-3/4" /></div> : data?.requests.length ? data.requests.map(request => <div key={request.id} className="flex items-start gap-2 border-b border-border/50 py-1.5"><span className="shrink-0 text-muted-foreground">{request.method}</span><span className={`shrink-0 ${request.failed || (request.status ?? 0) >= 400 ? "text-destructive" : "text-onedark-green"}`}>{request.failed ? "Falhou" : request.status ?? "…"}</span><span className="min-w-0 break-all text-muted-foreground">{request.url}</span></div>) : <p className="text-muted-foreground">Nenhuma requisição capturada. A captura começa ao vincular a aba.</p>}</div>
    {data && data.total > 25 && <div className="flex shrink-0 items-center justify-end gap-2 border-t border-border px-2 py-1"><span className="font-mono text-[10px] text-muted-foreground">{data.offset + 1}–{data.offset + data.requests.length} de {data.total}</span><Button variant="ghost" size="sm" disabled={busy || data.offset === 0} className="h-6 cursor-pointer text-xs" onClick={() => setQuery(current => ({ ...current, offset: Math.max(0, data.offset - 25) }))}>Anterior</Button><Button variant="ghost" size="sm" disabled={busy || data.offset + data.requests.length >= data.total} className="h-6 cursor-pointer text-xs" onClick={() => setQuery(current => ({ ...current, offset: data.offset + 25 }))}>Próxima</Button></div>}
  </section>;
}
