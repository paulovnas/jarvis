import { useEffect, useRef, useState } from "react";
import { AlertCircle, Gauge, RefreshCw, X } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Progress } from "@/components/ui/progress";
import { Sheet, SheetClose, SheetContent, SheetDescription, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { aliasSuffix, expectedQuotaRemaining, planLabel, quotaPercent, quotaReserve, remainingTime, type AccountUsage, type UsageWindow } from "@/core/provider-usage";
import { cn } from "@/lib/utils";
import { RemoteProviderIcon } from "./RemoteProviderIcon";

export type RemoteUsageAccount = AccountUsage & { providerKind: "openai-codex" | "antigravity" | "claude-code" | "opencode-go" };
const providerLabels = { "openai-codex": "OpenAI Codex", antigravity: "Antigravity", "claude-code": "Claude Code", "opencode-go": "OpenCode Go" };

function WindowUsage({ window, now, stale }: { window: UsageWindow; now: number; stale: boolean }) {
  const reset = remainingTime(window.resetsAt, now);
  const reserve = stale ? null : quotaReserve(window, now);
  const expected = stale ? null : expectedQuotaRemaining(window, now);
  const remaining = window.remainingPercent;
  const color = remaining === null ? "text-muted-foreground" : remaining <= 10 ? "text-onedark-red" : remaining <= 30 ? "text-onedark-yellow" : "text-onedark-green";
  return <section aria-label={`${window.group} ${window.label}`} className="flex min-w-0 flex-col gap-1.5">
    <p className="truncate font-mono text-[10px] text-muted-foreground" title={window.group}>{window.group}</p>
    <div className="flex min-w-0 items-center justify-between gap-2 font-mono text-xs tabular-nums"><span className="truncate" title={window.label}>{window.label}</span><span className={cn("shrink-0", color)}>{quotaPercent(remaining)}</span></div>
    {remaining !== null && <div className="relative">
      <Progress aria-label={`${window.group} ${window.label} restante`} value={remaining} className={cn("[&_[data-slot=progress-indicator]]:motion-reduce:transition-none", remaining <= 10 ? "[&_[data-slot=progress-indicator]]:bg-onedark-red" : remaining <= 30 ? "[&_[data-slot=progress-indicator]]:bg-onedark-yellow" : "[&_[data-slot=progress-indicator]]:bg-onedark-green")} />
      {expected !== null && <span role="img" aria-label={`Restante esperado: ${Math.round(expected)}%`} style={{ left: `${expected}%` }} className="absolute -top-0.5 h-2 w-0.5 -translate-x-1/2 rounded-full bg-foreground" />}
    </div>}
    {reset && <p className="font-mono text-[10px] text-muted-foreground">{reset === "agora" ? "Renovação prevista agora" : `Renova em ${reset}`}</p>}
    {reserve !== null && <p className={cn("font-mono text-[10px]", reserve < 0 ? "text-onedark-red" : reserve > 0 ? "text-onedark-green" : "text-muted-foreground")}>{reserve === 0 ? "No ritmo da janela" : `${Math.abs(reserve)}% ${reserve > 0 ? "em reserva" : "em déficit"}`}</p>}
  </section>;
}

function AccountCard({ account, now, failed }: { account: RemoteUsageAccount; now: number; failed: boolean }) {
  const stale = failed || Boolean(account.error) || !account.fetchedAt || now - account.fetchedAt > 5 * 60_000;
  return <Card size="sm" role="region" aria-label={`Limites de ${account.alias}`} className="min-w-0 shrink-0 gap-3 rounded-lg">
    <CardHeader>
      <CardTitle className="flex min-w-0 items-center gap-2"><span role="img" aria-label={providerLabels[account.providerKind]} className="flex"><RemoteProviderIcon kind={account.providerKind} /></span><span className="min-w-0 flex-1 truncate" title={account.alias}>{aliasSuffix(account.alias)}</span>{account.plan && <Badge variant="outline" className="max-w-24 shrink-0 truncate">{planLabel(account.plan, "unknown")}</Badge>}</CardTitle>
    </CardHeader>
    <CardContent className="flex min-w-0 flex-col gap-3">
      {account.error && <p role="status" className="text-xs text-onedark-yellow">{account.fetchedAt ? "Limites desatualizados" : "Limites indisponíveis"}</p>}
      {!account.error && !account.windows.length && <p className="text-xs text-muted-foreground">Nenhuma janela informada.</p>}
      <div className="grid min-w-0 grid-cols-2 gap-x-4 gap-y-3">{account.windows.map(window => <WindowUsage key={window.id} window={window} now={now} stale={stale} />)}</div>
      {account.resetCredits && <div className="flex items-center justify-between gap-3 text-xs"><span className="text-muted-foreground">Resets disponíveis</span><span className="font-mono text-primary">{account.resetCredits.availableCount}</span></div>}
    </CardContent>
    {account.fetchedAt !== null && <CardFooter className="justify-end border-t py-2"><time dateTime={new Date(account.fetchedAt).toISOString()} className="font-mono text-[10px] text-muted-foreground">Atualizado às {new Date(account.fetchedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</time></CardFooter>}
  </Card>;
}

/** Fetches sanitized quotas through the authenticated remote client, never native IPC. */
export function RemoteUsage({ load }: { load: (refresh?: boolean) => Promise<RemoteUsageAccount[]> }) {
  const [open, setOpen] = useState(false);
  const [accounts, setAccounts] = useState<RemoteUsageAccount[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [now, setNow] = useState(Date.now);
  const request = useRef(0);
  useEffect(() => () => { request.current += 1; }, []);
  useEffect(() => {
    if (!open) return;
    const timer = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, [open]);
  async function refresh(force = false) {
    const id = ++request.current;
    setBusy(true); setFailed(false);
    try {
      const value = await load(force);
      if (request.current === id) { setAccounts(value); setNow(Date.now()); }
    } catch {
      if (request.current === id) setFailed(true);
    } finally {
      if (request.current === id) setBusy(false);
    }
  }
  function changeOpen(value: boolean) {
    setOpen(value);
    if (value) void refresh(true);
    else { request.current += 1; setBusy(false); }
  }
  return <Sheet open={open} onOpenChange={changeOpen}>
    <SheetTrigger render={<Button variant="ghost" size="icon" aria-label="Limites dos provedores" className="size-11 shrink-0 cursor-pointer" />}><Gauge /></SheetTrigger>
    <SheetContent side="bottom" className="remote-sheet max-h-[85dvh] gap-0 motion-reduce:transition-none" showCloseButton={false}>
      <SheetHeader className="shrink-0 border-b">
        <div className="flex items-center justify-between gap-2"><SheetTitle>Limites dos provedores</SheetTitle><div className="flex shrink-0 items-center gap-1"><Button variant="ghost" size="icon" aria-label="Atualizar limites" aria-busy={busy} disabled={busy} className="size-11 cursor-pointer" onClick={() => void refresh(true)}><RefreshCw className={cn(busy && "motion-safe:animate-spin")} /></Button><SheetClose render={<Button variant="ghost" size="icon" aria-label="Fechar limites" className="size-11 cursor-pointer" />}><X /></SheetClose></div></div>
        <SheetDescription className="sr-only">Cotas restantes, renovação e ritmo de consumo dos provedores conectados.</SheetDescription>
      </SheetHeader>
      <div className="flex min-h-0 flex-col gap-3 overflow-y-auto p-4 pb-[max(16px,env(safe-area-inset-bottom))]">
        {failed && <Alert><AlertCircle /><AlertTitle>Não foi possível atualizar os limites.</AlertTitle><AlertDescription>{accounts ? "Os últimos dados disponíveis foram preservados. Tente atualizar novamente." : "Tente atualizar novamente."}</AlertDescription></Alert>}
        {!accounts && !failed && <CardsSkeleton label="Carregando limites" />}
        {accounts?.map(account => <AccountCard key={`${account.providerKind}/${account.alias}`} account={account} now={now} failed={failed} />)}
        {accounts?.length === 0 && <Empty><EmptyHeader><EmptyMedia variant="icon"><Gauge /></EmptyMedia><EmptyTitle>Nenhum limite disponível</EmptyTitle><EmptyDescription>Os provedores conectados não informaram limites de uso.</EmptyDescription></EmptyHeader></Empty>}
      </div>
    </SheetContent>
  </Sheet>;
}
