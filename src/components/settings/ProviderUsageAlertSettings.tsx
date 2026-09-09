import { useState } from "react";
import { BellRing } from "lucide-react";
import { Input } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import type { ProviderAccount, ProviderUsageAlert } from "@/core/provider-accounts";
import { availableUsageAlertWindows, type UsageAlertWindow } from "@/core/provider-usage";
import { useProviderUsage } from "@/hooks/use-provider-usage";

const WINDOW_LABELS: Record<UsageAlertWindow, string> = {
  five_hour: "5 horas",
  weekly: "Semanal",
};

export function ProviderUsageAlertSettings({ account, saving, onChange }: {
  account: ProviderAccount;
  saving: boolean;
  onChange: (alias: string, alert: ProviderUsageAlert | null) => void;
}) {
  const { data, error } = useProviderUsage(account.alias);
  const [threshold, setThreshold] = useState(String(account.usageAlert?.remainingPercent ?? 20));
  const [validationError, setValidationError] = useState<string | null>(null);

  const relevant = data?.windows.filter(window => account.providerKind !== "openai-codex" || window.group === "Codex") ?? [];
  const available = availableUsageAlertWindows(relevant, account.showThirdPartyUsage === true);
  const defaultWindow = available.includes("weekly") ? "weekly" : available[0];
  const selectedWindow = account.usageAlert?.window ?? defaultWindow;
  const selectedUnavailable = Boolean(account.usageAlert && data && !available.includes(account.usageAlert.window));

  const commitThreshold = () => {
    if (!account.usageAlert) return;
    const value = Number(threshold);
    if (!Number.isInteger(value) || value < 1 || value > 100) {
      setValidationError("Informe um número inteiro entre 1 e 100.");
      setThreshold(String(account.usageAlert.remainingPercent));
      return;
    }
    setValidationError(null);
    if (value !== account.usageAlert.remainingPercent) {
      onChange(account.alias, { ...account.usageAlert, remainingPercent: value });
    }
  };

  return <section aria-label={`Alertas de limite de ${account.alias}`} className="rounded-lg border border-border bg-muted/25 p-3 shadow-[inset_0_1px_0_#ffffff0d]">
    <div className="flex items-center justify-between gap-3">
      <Label htmlFor={`usage-alert-${account.alias}`} className="flex min-w-0 items-center gap-2 text-xs">
        <BellRing aria-hidden="true" className="size-4 shrink-0 text-onedark-yellow" />
        Alertar sobre limite
      </Label>
      <Switch
        id={`usage-alert-${account.alias}`}
        aria-label={`Alertar sobre limites de ${account.alias}`}
        checked={Boolean(account.usageAlert)}
        disabled={saving || (!account.usageAlert && !defaultWindow)}
        onCheckedChange={enabled => onChange(account.alias, enabled && defaultWindow ? { window: defaultWindow, remainingPercent: Number(threshold) || 20 } : null)}
        className="cursor-pointer"
      />
    </div>
    {!data && !error ? <div role="status" aria-label="Consultando janelas de limite" className="mt-3 grid grid-cols-2 gap-2"><Skeleton className="h-9" /><Skeleton className="h-9" /></div> : account.usageAlert ? <div className="mt-3 grid gap-3 sm:grid-cols-2">
      <div className="space-y-1.5">
        <Label htmlFor={`usage-alert-window-${account.alias}`} className="text-[11px] text-muted-foreground">Janela</Label>
        <Select value={selectedWindow} disabled={saving || available.length === 0} onValueChange={value => {
          if (value === "five_hour" || value === "weekly") onChange(account.alias, { ...account.usageAlert!, window: value });
        }}>
          <SelectTrigger id={`usage-alert-window-${account.alias}`} className="w-full cursor-pointer text-xs"><SelectValue>{selectedWindow ? WINDOW_LABELS[selectedWindow] : "Indisponível"}</SelectValue></SelectTrigger>
          <SelectContent>
            {available.map(window => <SelectItem key={window} value={window} className="cursor-pointer text-xs">{WINDOW_LABELS[window]}</SelectItem>)}
          </SelectContent>
        </Select>
      </div>
      <div className="space-y-1.5">
        <Label htmlFor={`usage-alert-threshold-${account.alias}`} className="text-[11px] text-muted-foreground">Avisar quando restar</Label>
        <div className="relative">
          <Input id={`usage-alert-threshold-${account.alias}`} aria-label={`Porcentagem restante para ${account.alias}`} type="number" min={1} max={100} step={1} inputMode="numeric" value={threshold} disabled={saving} onChange={event => setThreshold(event.target.value)} onBlur={commitThreshold} onKeyDown={event => { if (event.key === "Enter") event.currentTarget.blur(); }} className="pr-8 font-mono text-xs tabular-nums" />
          <span aria-hidden="true" className="pointer-events-none absolute inset-y-0 right-3 flex items-center font-mono text-xs text-muted-foreground">%</span>
        </div>
      </div>
    </div> : data && available.length === 0 ? <p className="mt-2 text-[11px] text-muted-foreground">Esta conta não informou uma janela de 5 horas ou semanal.</p> : null}
    {error && !data && <p role="status" className="mt-2 text-[11px] text-onedark-yellow">Não foi possível consultar as janelas agora.</p>}
    {selectedUnavailable && <p role="status" className="mt-2 text-[11px] text-onedark-yellow">A janela configurada não apareceu na consulta mais recente.</p>}
    {validationError && <p role="alert" className="mt-2 text-[11px] text-destructive">{validationError}</p>}
    <p className="mt-2 text-[10px] leading-relaxed text-muted-foreground">O aviso usa o percentual restante e depende das notificações do sistema na aba Geral.</p>
  </section>;
}
