import { type CSSProperties } from "react";
import { AlertCircle } from "lucide-react";
import { ProviderIcon } from "@/components/ProviderIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { useProviderUsage } from "@/hooks/use-provider-usage";
import type { ProviderAccount } from "@/core/provider-accounts";
import { aliasSuffix, planLabel, remainingTime, quotaColor, quotaPercent, quotaReserve, type UsageWindow } from "@/core/provider-usage";

function WindowBar({ window, now, stale }: { window: UsageWindow; now: number; stale: boolean }) {
  const reset = remainingTime(window.resetsAt, now);
  const reserve = stale ? null : quotaReserve(window, now);
  return <div className="space-y-2">
    <div className="flex items-center justify-between gap-5 font-mono text-[11px] tabular-nums"><span>{window.label}</span><span style={{ color: quotaColor(window.remainingPercent) }}>{quotaPercent(window.remainingPercent)}</span></div>
    {window.remainingPercent !== null && <Progress aria-label={`${window.group} ${window.label} restante`} value={window.remainingPercent} style={{ "--quota-color": quotaColor(window.remainingPercent) } as CSSProperties} className="[&_[data-slot=progress-indicator]]:bg-[var(--quota-color)]" />}
    {reset && <p title={window.resetsAt ? new Date(window.resetsAt).toLocaleString("pt-BR") : undefined} className="text-[10px] text-muted-foreground">{reset === "agora" ? "Reset previsto agora" : `Renova em ${reset}`}</p>}
    {reserve !== null && <p title="Saldo em pontos percentuais comparado ao consumo uniforme ao longo da janela." className={`font-mono text-[10px] ${reserve < 0 ? "text-onedark-red" : reserve > 0 ? "text-onedark-green" : "text-muted-foreground"}`}>{reserve === 0 ? "No ritmo da janela" : `${Math.abs(reserve)} p.p. ${reserve > 0 ? "em reserva" : "em déficit"}`}</p>}
  </div>;
}

export function ProviderUsage({ account, now }: { account: ProviderAccount; now: number }) {
  const { data, error } = useProviderUsage(account.alias);
  const windows = data?.windows.filter(window => account.providerKind !== "antigravity" || !window.thirdParty || account.showThirdPartyUsage === true) ?? [];
  const groups = [...new Set(windows.map(window => window.group))];
  const failed = error || Boolean(data?.error);
  const compact = account.providerKind === "openai-codex" ? windows.filter(window => window.group === "Codex") : windows;
  const credits = data?.resetCredits;
  const suffix = aliasSuffix(account.alias);
  return <Popover>
    <PopoverTrigger render={<Button variant="ghost" />} aria-label={`Limites de ${account.alias}`} openOnHover delay={250} closeDelay={150} className="h-6 shrink-0 cursor-pointer gap-1.5 rounded-sm px-2 font-mono text-[10px] font-normal tabular-nums">
      <ProviderIcon kind={account.providerKind} />
      <span className="max-w-24 truncate">{suffix}</span>
      {!data && !error ? <Skeleton aria-label={`Carregando limites de ${suffix}`} className="h-3 w-28" /> : compact.length ? compact.slice(0, 4).map((window, index) => <span key={window.id} className="flex items-center gap-1">
        {index > 0 && <span aria-hidden="true" className="mx-1 text-muted-foreground/50">|</span>}
        {account.providerKind === "antigravity" && groups.length > 1 && <span className="text-muted-foreground">{window.thirdParty ? "3P" : "Gemini"}</span>}
        <span className="text-muted-foreground">{window.label}</span><span style={{ color: quotaColor(window.remainingPercent) }}>{quotaPercent(window.remainingPercent)}</span>
        {window.resetsAt !== null && <span className="text-muted-foreground">({remainingTime(window.resetsAt, now)})</span>}
      </span>) : <span className="text-muted-foreground">—</span>}
      {compact.length > 4 && <span className="text-muted-foreground">+{compact.length - 4}</span>}
      {failed && <AlertCircle aria-label="Limites desatualizados" className="size-3 text-onedark-yellow" />}
    </PopoverTrigger>
    <PopoverContent aria-label={`Limites de ${account.alias}`} initialFocus={false} side="top" align="end" sideOffset={10} className="dark instrument-panel max-h-[70vh] w-[360px] max-w-[90vw] overflow-y-auto bg-card p-4 gap-0 text-foreground">
      <div className="mb-4 flex items-start justify-between gap-3 border-b border-border pb-3"><div className="min-w-0"><p className="truncate text-xs font-medium">{data?.email ?? account.email ?? account.alias}</p><p className="mt-1 font-mono text-[10px] text-muted-foreground">{account.providerKind === "antigravity" ? "Antigravity" : "OpenAI Codex"} · {suffix}</p></div><Badge variant="outline" className="max-w-36 shrink-0 truncate text-[10px]">{planLabel(data?.plan, account.accountType)}</Badge></div>
      {!data && !error && <div role="status" aria-label="Carregando limites" className="space-y-4"><Skeleton className="h-5" /><Skeleton className="h-2" /><Skeleton className="h-5" /><Skeleton className="h-2" /></div>}
      {failed && <p role="status" className="mb-3 text-[11px] text-onedark-yellow">{data?.fetchedAt ? "Limites desatualizados" : "Limites indisponíveis"}</p>}
      {data && !windows.length && !failed && <p className="text-xs text-muted-foreground">Nenhuma janela informada.</p>}
      <div className={`grid gap-5 ${groups.length > 1 ? "grid-cols-2" : "grid-cols-1"}`}>{groups.map(group => <section key={group} className="min-w-0 space-y-4"><h3 className="micro-label truncate text-muted-foreground" title={group}>{group}</h3>{windows.filter(window => window.group === group).map(window => <WindowBar key={window.id} window={window} now={now} stale={Boolean(failed) || !data?.fetchedAt || now - data.fetchedAt > 5 * 60_000} />)}</section>)}</div>
      {credits && <section className="mt-4 border-t border-border pt-3"><div className="flex items-center justify-between text-xs"><span>Resets disponíveis</span><span className="font-mono text-primary">{credits.availableCount}</span></div>{credits.availableCount > 0 && <div className="mt-2 space-y-1 text-[10px] text-muted-foreground">{credits.detailsAvailable && credits.expirations.length ? credits.expirations.map((expiry, index) => <p key={index}>{expiry ? `Expira em ${new Date(expiry).toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" })}` : "Validade não informada"}</p>) : <p>Validade indisponível</p>}</div>}</section>}
      {data?.fetchedAt && <p className="mt-4 font-mono text-[9px] text-muted-foreground/70">Atualizado às {new Date(data.fetchedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</p>}
    </PopoverContent>
  </Popover>;
}
