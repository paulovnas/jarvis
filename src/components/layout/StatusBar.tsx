import { lazy, Suspense, useEffect, useState } from "react";
import { Separator } from "@/components/ui/separator";
import { Settings } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { ProviderAccount } from "@/core/provider-accounts";
import { Skeleton } from "@/components/ui/skeleton";

const ProviderUsage = lazy(() => import("./ProviderUsage").then(module => ({ default: module.ProviderUsage })));

export function StatusBar({ accounts = [], onOpenSettings }: { accounts?: ProviderAccount[]; onOpenSettings?: () => void }) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      clearTimeout(timer);
      setNow(new Date());
      timer = setTimeout(tick, 60_000 - Date.now() % 60_000);
    };
    tick();
    window.addEventListener("focus", tick);
    return () => { clearTimeout(timer); window.removeEventListener("focus", tick); };
  }, []);
  return <footer aria-label="Barra de status" className="shrink-0 bg-sidebar">
    <Separator />
    <div className="flex h-7 min-w-0 items-center gap-2 px-2">
      {onOpenSettings && <Button variant="ghost" size="icon-sm" className="h-6 w-7 shrink-0 cursor-pointer rounded-sm text-muted-foreground" aria-label="Configurações" title="Configurações" onClick={onOpenSettings}><Settings className="size-3.5" /></Button>}
      <div aria-label="Limites dos provedores" className="ml-auto flex min-w-0 flex-row-reverse items-center overflow-x-auto">{accounts.filter(account => account.enabled && account.providerKind !== "custom" && account.showUsage !== false).map(account => <Suspense key={`${account.alias}/${account.createdAt}`} fallback={<Skeleton aria-label={`Carregando limites de ${account.alias}`} className="mx-2 h-3 w-44 shrink-0" />}><ProviderUsage account={account} now={now.getTime()} /></Suspense>)}</div>
      <time aria-label="Hora atual" dateTime={now.toISOString()} title={now.toLocaleDateString("pt-BR", { dateStyle: "full" })} className="shrink-0 border-l border-border px-2 font-mono text-[10px] tabular-nums text-muted-foreground">
        {now.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}
      </time>
    </div>
  </footer>;
}
