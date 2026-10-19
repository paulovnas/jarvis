import { providerDisplayName } from "@/core/provider-accounts";
import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Separator } from "@/components/ui/separator";
import { Gauge, Settings } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { ProviderAccount } from "@/core/provider-accounts";
import { Skeleton } from "@/components/ui/skeleton";
import { useProviderUsage } from "@/hooks/use-provider-usage";
import { Hint } from "@/components/ui/hint";
import { Popover, PopoverContent, PopoverTitle, PopoverTrigger } from "@/components/ui/popover";
import { RowsSkeleton } from "./LoadingSkeletons";

const ProviderUsage = lazy(() => import("./ProviderUsage").then(module => ({ default: module.ProviderUsage })));
const ClaudeUsage = lazy(() => import("./ProviderUsage").then(module => ({ default: module.ClaudeUsage })));
const MuseUsage = lazy(() => import("./ProviderUsage").then(module => ({ default: module.MuseUsage })));
const AppUpdate = lazy(() => import("./AppUpdate").then(module => ({ default: module.AppUpdate })));
const ResourceUpdates = lazy(() => import("./ResourceUpdates").then(module => ({ default: module.ResourceUpdates })));
const RemoteAccess = lazy(() => import("../remote/RemoteAccess").then(module => ({ default: module.RemoteAccess })));

function UsageAlertMonitor({ account }: { account: ProviderAccount }) {
  useProviderUsage(account.alias, { pollWhileHidden: true });
  return null;
}

function ProviderLimits({ accounts, now, passive, presentation = "compact", inactive = false }: { accounts: ProviderAccount[]; now: number; passive: boolean; presentation?: "compact" | "details"; inactive?: boolean }) {
  return <>{accounts.filter(account => account.enabled && account.providerKind !== "custom" && account.showUsage !== false).map(account => <Suspense key={`${account.alias}/${account.createdAt}`} fallback={presentation === "details" ? <RowsSkeleton label={`Carregando limites de ${providerDisplayName(account)}`} count={1} /> : <Skeleton aria-label={`Carregando limites de ${providerDisplayName(account)}`} className="mx-2 h-3 w-44 shrink-0" />}><ProviderUsage account={account} now={now} presentation={presentation} inactive={inactive} /></Suspense>)}{!passive && <Suspense fallback={null}><ClaudeUsage now={now} presentation={presentation} inactive={inactive} /><MuseUsage now={now} presentation={presentation} inactive={inactive} /></Suspense>}</>;
}

export function StatusBar({ accounts = [], onOpenSettings, passive = false }: { accounts?: ProviderAccount[]; onOpenSettings?: () => void; passive?: boolean }) {
  const [now, setNow] = useState(() => new Date());
  const [collapsed, setCollapsed] = useState(false);
  const limitsRef = useRef<HTMLDivElement>(null);
  const stripRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const limits = limitsRef.current, strip = stripRef.current;
    if (!limits || !strip) return;
    const measure = () => setCollapsed(strip.getBoundingClientRect().width > limits.getBoundingClientRect().width);
    const observer = new ResizeObserver(measure);
    observer.observe(limits);
    observer.observe(strip);
    measure();
    return () => observer.disconnect();
  }, []);
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
  return <footer aria-label="Barra de status" className="native-statusbar shrink-0 bg-sidebar">
    {accounts.filter(account => account.enabled && account.providerKind !== "custom" && account.showUsage === false && account.usageAlert).map(account => <UsageAlertMonitor key={`alert/${account.alias}/${account.createdAt}`} account={account} />)}
    <Separator />
    <div className="flex h-7 min-w-0 items-center gap-2 px-3">
      {onOpenSettings && <Hint content="Configurações"><Button variant="ghost" size="icon-sm" className="size-6 shrink-0 cursor-pointer rounded-md text-muted-foreground" aria-label="Configurações" onClick={onOpenSettings}><Settings className="size-3.5" /></Button></Hint>}
      {!passive && <Suspense fallback={null}><RemoteAccess /></Suspense>}
      {!passive && <Suspense fallback={null}><AppUpdate /></Suspense>}
      {!passive && <Suspense fallback={null}><ResourceUpdates /></Suspense>}
      <div ref={limitsRef} role="group" aria-label="Limites dos provedores" className="relative ml-auto flex h-full min-w-0 flex-1 items-center justify-end overflow-hidden">
        <div ref={stripRef} inert={collapsed} aria-hidden={collapsed || undefined} className={`absolute inset-y-0 right-0 flex w-max flex-row-reverse items-center ${collapsed ? "invisible" : ""}`}>
          <ProviderLimits accounts={accounts} now={now.getTime()} passive={passive} inactive={collapsed} />
        </div>
        {collapsed && <Popover>
          <PopoverTrigger render={<Button variant="ghost" />} openOnHover delay={250} closeDelay={150} className="h-6 shrink-0 cursor-pointer gap-1.5 rounded-md px-2 text-[11px] font-normal text-muted-foreground"><Gauge className="size-3.5" />Limites</PopoverTrigger>
          <PopoverContent aria-label="Limites dos provedores" initialFocus={false} side="top" align="end" sideOffset={10} className="max-h-[70vh] w-[360px] max-w-[90vw] gap-5 overflow-x-hidden overflow-y-auto bg-card p-4 text-foreground motion-reduce:animate-none">
            <PopoverTitle className="text-xs">Limites dos provedores</PopoverTitle>
            <ProviderLimits accounts={accounts} now={now.getTime()} passive={passive} presentation="details" />
          </PopoverContent>
        </Popover>}
      </div>
      <Hint content={now.toLocaleDateString("pt-BR", { dateStyle: "full" })}><time aria-label="Hora atual" dateTime={now.toISOString()} className="shrink-0 border-l border-border px-2 font-mono text-[11px] tabular-nums text-muted-foreground">
        {now.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}
      </time></Hint>
    </div>
  </footer>;
}
