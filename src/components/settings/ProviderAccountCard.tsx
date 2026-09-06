import { useId } from "react";
import { CheckCircle2, ChevronRight, Unplug } from "lucide-react";
import { ProviderIcon } from "@/components/ProviderIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import type { ProviderAccount } from "@/core/provider-accounts";

const ACCOUNT_TYPE_LABELS: Record<ProviderAccount["accountType"], string> = {
  personal: "Pessoal",
  enterprise: "Enterprise",
  unknown: "Não identificado",
};

function formatConnectionDate(timestamp: number): string {
  if (!timestamp || timestamp <= 0) return "Data indisponível";
  return new Date(timestamp * 1000).toLocaleDateString("pt-BR", {
    day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit",
  });
}

export function ProviderAccountCard({ account, onDisconnect, onEnabledChange, onUsageChange, saving = false }: {
  account: ProviderAccount;
  onDisconnect: (alias: string) => void;
  onEnabledChange: (alias: string, enabled: boolean) => void;
  onUsageChange?: (alias: string, showUsage: boolean, showThirdPartyUsage: boolean) => void;
  saving?: boolean;
}) {
  const summaryId = useId();
  const modelSummary = !account.enabled ? "Desativada" : !account.modelsAvailable
    ? "Modelos indisponíveis"
    : account.models.length === 0
      ? "Nenhum modelo"
      : `${account.models.length} ${account.models.length === 1 ? "modelo" : "modelos"}`;

  return (
    <Dialog><Card size="sm"
      data-testid={`provider-account-${account.alias}`}
      className="min-w-0 gap-0 py-0"
    >
      <DialogTrigger
        render={<CardHeader />}
        nativeButton={false}
        aria-label={`Detalhes de ${account.alias}`}
        aria-describedby={summaryId}
        className="group cursor-pointer rounded-lg py-3 transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
      >
        <div className="flex min-w-0 items-center gap-3">
          <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground">
            <ProviderIcon kind={account.providerKind} className="size-4" />
          </div>
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <CardTitle className="min-w-0 truncate font-mono text-xs!" title={account.alias}>{account.alias}</CardTitle>
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <Badge variant="outline" className={`shrink-0 ${!account.enabled ? "text-muted-foreground" : !account.modelsAvailable ? "border-[#e5c07b]/30 bg-[#e5c07b]/10 text-[#e5c07b]" : "border-[#98c379]/30 bg-[#98c379]/10 text-[#98c379]"}`}>
                <CheckCircle2 aria-hidden="true" data-icon="inline-start" />
                {!account.enabled ? "Desativada" : account.modelsAvailable ? "Conectada" : "Indisponível"}
              </Badge>
            <CardDescription id={summaryId} className="text-xs">
              {account.providerKind === "antigravity" ? "Antigravity" : "OpenAI Codex"} · {modelSummary}
            </CardDescription>
            </div>
          </div>
          <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
        </div>
      </DialogTrigger>
      </Card><DialogContent className="max-h-[80vh] overflow-y-auto sm:max-w-xl" aria-describedby={undefined}>
        <DialogHeader><DialogTitle className="flex min-w-0 items-center gap-2 pr-6 text-sm"><ProviderIcon kind={account.providerKind} /><span className="truncate">{account.alias}</span></DialogTitle></DialogHeader>
        <CardContent className="flex flex-col gap-3 pb-1">
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2">
            <dt className="text-muted-foreground">E-mail</dt>
            <dd className="break-all text-right">{account.email ?? "Não informado"}</dd>
            <dt className="text-muted-foreground">Tipo de conta</dt>
            <dd className="text-right">{ACCOUNT_TYPE_LABELS[account.accountType]}</dd>
            <dt className="text-muted-foreground">Conectada em</dt>
            <dd className="text-right">{formatConnectionDate(account.createdAt)}</dd>
          </dl>
          <Separator />
          <div className="space-y-3">
            <label className="flex cursor-pointer items-center justify-between gap-3 text-xs">
              <span aria-hidden="true">Limites na statusbar</span>
              <Switch aria-label={`Limites de ${account.alias} na statusbar`} checked={account.showUsage !== false} disabled={saving} onCheckedChange={show => onUsageChange?.(account.alias, show, account.showThirdPartyUsage === true)} className="cursor-pointer" />
            </label>
            {account.providerKind === "antigravity" && <label className="flex cursor-pointer items-center justify-between gap-3 text-xs">
              <span aria-hidden="true">Incluir modelos de terceiros</span>
              <Switch aria-label={`Incluir modelos de terceiros de ${account.alias}`} checked={account.showThirdPartyUsage === true} disabled={saving} onCheckedChange={show => onUsageChange?.(account.alias, account.showUsage !== false, show)} className="cursor-pointer" />
            </label>}
          </div>
          <Separator />
          <div className="flex flex-col gap-2">
            <span className="font-medium">Modelos disponíveis</span>
            {!account.enabled ? <p className="text-muted-foreground">Ative a conta para disponibilizar seus modelos.</p> : !account.modelsAvailable ? (
              <p className="text-muted-foreground">Não foi possível consultar os modelos agora.</p>
            ) : account.models.length === 0 ? (
              <p className="text-muted-foreground">A assinatura não retornou modelos.</p>
            ) : (
              <div className="flex flex-wrap gap-1.5">
                {account.models.map((model) => (
                  <Badge key={model.id} variant="outline" title={model.id}>{model.name}</Badge>
                ))}
              </div>
            )}
          </div>
        </CardContent>
        <CardFooter className="flex-wrap justify-between gap-3">
          <label className="flex cursor-pointer items-center gap-2 text-xs">
            <Switch aria-label={`Ativar ${account.alias}`} checked={account.enabled} onCheckedChange={(enabled) => onEnabledChange(account.alias, enabled)} disabled={saving} className="cursor-pointer" />
            <span aria-hidden="true">{account.enabled ? "Ativada" : "Desativada"}</span>
          </label>
          <Button type="button" variant="destructive" size="sm" disabled={saving} onClick={() => onDisconnect(account.alias)} className="cursor-pointer">
            <Unplug aria-hidden="true" data-icon="inline-start" />
            Desconectar
          </Button>
        </CardFooter>
      </DialogContent>
    </Dialog>
  );
}
