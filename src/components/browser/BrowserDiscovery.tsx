import { useState } from "react";
import { Link2, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Hint } from "@/components/ui/hint";
import { Skeleton } from "@/components/ui/skeleton";
import { browserDiscoverySchema } from "@/core/browser";
import type { BrowserController } from "@/hooks/use-browser";

type AvailableTab = { id: string; title: string; url: string; owned: boolean };

export function BrowserDiscovery({ browser }: { browser: BrowserController }) {
  const [open, setOpen] = useState(false);
  const [tabs, setTabs] = useState<AvailableTab[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const discover = async () => {
    setBusy(true); setFailed(false);
    try {
      const value = await browser.command({ action: "discover" });
      const parsed = browserDiscoverySchema.safeParse(value);
      if (parsed.success) setTabs(parsed.data.tabs);
      else { setFailed(true); if (value !== undefined) toast.error("Não foi possível listar as abas do navegador."); }
    } finally { setBusy(false); }
  };
  const attach = async (id: string) => {
    setBusy(true);
    try { if (await browser.command({ action: "attach", id }) !== undefined) setOpen(false); } finally { setBusy(false); }
  };
  return <>
    <Hint content="Vincular uma aba do navegador a este chat"><Button variant="ghost" size="icon" aria-label="Vincular aba externa" className="size-6 shrink-0 cursor-pointer" onClick={() => { setOpen(true); void discover(); }}><Link2 className="size-3.5" /></Button></Hint>
    <Dialog open={open} onOpenChange={setOpen}><DialogContent className="max-h-[80vh] overflow-hidden sm:max-w-xl"><DialogHeader><DialogTitle>Abas do navegador</DialogTitle><DialogDescription>Selecione a aba que os agentes deste chat poderão usar. Outras conversas não assumem o controle dela.</DialogDescription></DialogHeader>
      <div className="flex items-center justify-end"><Button variant="ghost" size="sm" disabled={busy} className="cursor-pointer" onClick={() => void discover()}><RefreshCw className="size-3.5" />Atualizar abas</Button></div>
      <div className="max-h-[50vh] space-y-2 overflow-y-auto pr-2">
        {busy && tabs === null ? <div role="status" aria-label="Buscando abas externas" className="space-y-2"><Skeleton className="h-16" /><Skeleton className="h-16" /></div> : failed ? <p role="alert" className="text-xs text-muted-foreground">Não foi possível consultar o navegador. Verifique a conexão da extensão nas configurações.</p> : !tabs?.length ? <p className="text-xs text-muted-foreground">Nenhuma aba HTTP ou HTTPS disponível. Abra uma página no navegador e atualize a lista.</p> : tabs.map(tab => <Button key={tab.id} variant="outline" disabled={busy || tab.owned} onClick={() => void attach(tab.id)} className="h-auto w-full cursor-pointer justify-start whitespace-normal px-3 py-3 text-left"><span className="min-w-0 flex-1"><span className="block truncate text-xs font-medium">{tab.title || "Sem título"}</span><span className="mt-1 block truncate font-mono text-[10px] text-muted-foreground">{tab.url}</span>{tab.owned && <span className="mt-1 block text-[10px] text-muted-foreground">Já vinculada a uma conversa</span>}</span></Button>)}
      </div>
    </DialogContent></Dialog>
  </>;
}
