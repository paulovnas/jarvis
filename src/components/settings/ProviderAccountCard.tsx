import { useEffect, useId, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle2, ChevronRight, Pencil, RefreshCw, Unplug } from "lucide-react";
import { protocolLabels } from "@/core/custom-provider";
import { ProviderIcon } from "@/components/ProviderIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import type { ProviderAccount, ProviderUsageAlert } from "@/core/provider-accounts";
import { ProviderUsageAlertSettings } from "./ProviderUsageAlertSettings";
import { ProviderTransportSettings } from "./ProviderTransportSettings";
import { Hint } from "@/components/ui/hint";
import { enabledModels } from "@/core/provider-accounts";
import { providerReferencesSchema, type ProviderReference } from "@/core/provider-references";

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

export function ProviderAccountCard({ account, onDisconnect, onEnabledChange, onUsageChange, onUsageAlertChange, onModelEnabledChange, onRefreshModels, refreshing = false, onReviewAgents, onEdit, onReauthorize, saving = false }: {
  account: ProviderAccount;
  onDisconnect: (alias: string) => void;
  onEnabledChange: (alias: string, enabled: boolean) => void;
  onUsageChange?: (alias: string, showUsage: boolean, showThirdPartyUsage: boolean) => void;
  onUsageAlertChange?: (alias: string, alert: ProviderUsageAlert | null) => void;
  onModelEnabledChange?: (alias: string, modelId: string, enabled: boolean) => void;
  onRefreshModels?: (alias: string) => void;
  refreshing?: boolean;
  onReviewAgents?: (tab: "flows" | "agents") => void;
  saving?: boolean;
  onEdit?: (account: ProviderAccount) => void;
  onReauthorize?: (account: ProviderAccount) => void;
}) {
  const [open, setOpen] = useState(false);
  const [references, setReferences] = useState<ProviderReference[] | null>(null);
  const [referenceError, setReferenceError] = useState(false);
  useEffect(() => {
    if (!open) return;
    let active = true;
    void Promise.resolve().then(() => invoke("get_provider_model_references")).then(value => {
      if (active) { setReferences(providerReferencesSchema.parse(value).references); setReferenceError(false); }
    }).catch(() => { if (active) setReferenceError(true); });
    return () => { active = false; };
  }, [open]);
  const custom = account.providerKind === "custom";
  const summaryId = useId();
  const activeModels = enabledModels(account);
  const activeModelIds = new Set(activeModels.map(model => model.id));
  const affectedAgents = account.modelsAvailable ? (references ?? []).filter(reference =>
    reference.choice.account === account.alias
    && (reference.kind === "builtin_agent" || reference.kind === "custom_agent")
    && !activeModelIds.has(reference.choice.model),
  ) : [];
  const modelSummary = !account.enabled ? "Desativada" : !account.modelsAvailable
    ? "Modelos indisponíveis"
    : account.models.length === 0
      ? "Nenhum modelo"
      : `${activeModels.length} ${activeModels.length === 1 ? "modelo ativo" : "modelos ativos"}`;

  return (
    <Dialog open={open} onOpenChange={setOpen}><Card size="sm"
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
            <Hint content={account.alias} whenTruncated><CardTitle className="min-w-0 truncate font-mono text-xs!">{account.alias}</CardTitle></Hint>
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <Badge variant="outline" className={`shrink-0 ${!account.enabled ? "text-muted-foreground" : !account.modelsAvailable ? "border-[#e5c07b]/30 bg-[#e5c07b]/10 text-[#e5c07b]" : "border-[#98c379]/30 bg-[#98c379]/10 text-[#98c379]"}`}>
                <CheckCircle2 aria-hidden="true" data-icon="inline-start" />
                {!account.enabled ? "Desativada" : account.modelsAvailable ? custom ? "Configurada" : "Conectada" : "Indisponível"}
              </Badge>
            <CardDescription id={summaryId} className="text-xs">
              {custom ? "Custom" : account.providerKind === "antigravity" ? "Antigravity" : "OpenAI Codex"} · {modelSummary}
            </CardDescription>
            </div>
          </div>
          <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
        </div>
      </DialogTrigger>
      </Card><DialogContent className="flex max-h-[80vh] flex-col overflow-hidden sm:max-w-xl" aria-describedby={undefined}>
        <DialogHeader className="shrink-0">
          <DialogTitle className="flex min-w-0 items-center gap-3 pr-6">
            <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"><ProviderIcon kind={account.providerKind} className="size-4" /></span>
            <span className="min-w-0 break-words">{account.alias}</span>
          </DialogTitle>
        </DialogHeader>
        <div role="region" aria-label={`Configurações de ${account.alias}`} className="flex min-h-0 min-w-0 flex-col gap-4 overflow-x-hidden overflow-y-auto py-1">
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-4 gap-y-3 text-xs">
            {custom ? <><dt className="text-muted-foreground">Endpoint</dt><dd className="text-right">{account.custom ? protocolLabels[account.custom.protocol] : "Indisponível"}</dd><dt className="text-muted-foreground">URL base</dt><dd className="break-all text-right font-mono text-xs">{account.custom?.baseUrl}</dd></> : <><dt className="text-muted-foreground">E-mail</dt>
            <dd className="break-all text-right">{account.email ?? "Não informado"}</dd>
            <dt className="text-muted-foreground">Tipo de conta</dt>
            <dd className="text-right">{ACCOUNT_TYPE_LABELS[account.accountType]}</dd></>}
            <dt className="text-muted-foreground">{custom ? "Cadastrada em" : "Conectada em"}</dt>
            <dd className="text-right">{formatConnectionDate(account.createdAt)}</dd>
          </dl>
          {!custom && <><Separator /><div className="flex flex-col gap-3">
            <label className="flex cursor-pointer items-center justify-between gap-3 text-xs">
              <span aria-hidden="true">Limites na statusbar</span>
              <Switch aria-label={`Limites de ${account.alias} na statusbar`} checked={account.showUsage !== false} disabled={saving} onCheckedChange={show => onUsageChange?.(account.alias, show, account.showThirdPartyUsage === true)} className="cursor-pointer" />
            </label>
            {account.providerKind === "antigravity" && <label className="flex cursor-pointer items-center justify-between gap-3 text-xs">
              <span aria-hidden="true">Incluir modelos de terceiros</span>
              <Switch aria-label={`Incluir modelos de terceiros de ${account.alias}`} checked={account.showThirdPartyUsage === true} disabled={saving} onCheckedChange={show => onUsageChange?.(account.alias, account.showUsage !== false, show)} className="cursor-pointer" />
            </label>}
          </div><ProviderUsageAlertSettings key={`${account.usageAlert?.window ?? "off"}/${account.usageAlert?.remainingPercent ?? 20}`} account={account} saving={saving || !onUsageAlertChange} onChange={(alias, alert) => onUsageAlertChange?.(alias, alert)} /></>}
          <Separator />
          <div className="flex flex-col gap-2">
            <div className="flex flex-wrap items-center justify-between gap-2"><div className="flex items-center gap-2"><span className="micro-label text-muted-foreground">Modelos para seleção</span><Badge variant="secondary">{activeModels.length}/{account.models.length}</Badge></div>{!custom && <Button type="button" size="sm" variant="outline" disabled={saving || refreshing || !account.enabled || !onRefreshModels} onClick={() => onRefreshModels?.(account.alias)} className="cursor-pointer"><RefreshCw aria-hidden="true" data-icon="inline-start" className={refreshing ? "animate-spin" : undefined} />{refreshing ? "Consultando…" : "Atualizar modelos"}</Button>}</div>
            <p className="text-xs leading-5 text-muted-foreground">{custom ? "Escolha quais modelos cadastrados neste endpoint poderão ser usados. Para mudar o catálogo, edite a conta Custom." : "A consulta pode demorar. Modelos novos são ativados automaticamente. Modelos retirados pelo provedor deixam de aparecer; agentes que ainda os usam precisam de outro modelo."}</p>
            {!account.enabled ? <p className="text-muted-foreground">Ative a conta para disponibilizar seus modelos.</p> : !account.modelsAvailable ? (
              <p className="text-muted-foreground">Não foi possível consultar os modelos agora.</p>
            ) : account.models.length === 0 ? (
              <p className="text-muted-foreground">{custom ? "Nenhum modelo cadastrado." : "A assinatura não retornou modelos."}</p>
            ) : (
              <div className="min-w-0 divide-y divide-border/70 rounded-md border border-border/70">
                {account.models.map((model) => (
                  <div key={model.id} className="flex min-w-0 items-center justify-between gap-3 px-3 py-2.5 text-xs">
                    <span className="min-w-0 flex-1"><span className="block break-words font-medium text-foreground">{model.name}</span><span className="block break-all font-mono text-[10px] text-muted-foreground">{model.id}{model.contextWindow ? ` · ${model.contextWindow.toLocaleString("pt-BR")} tokens` : ""}</span></span>
                    <Switch aria-label={`Disponibilizar ${model.name}`} checked={activeModelIds.has(model.id)} disabled={saving || refreshing || !onModelEnabledChange} onCheckedChange={enabled => onModelEnabledChange?.(account.alias, model.id, enabled)} className="cursor-pointer" />
                  </div>
                ))}
              </div>
            )}
          </div>
          {affectedAgents.length > 0 && <div role="alert" className="space-y-2 rounded-md border border-onedark-yellow/30 bg-onedark-yellow/5 p-3 text-xs"><p className="font-medium text-foreground">{affectedAgents.length} {affectedAgents.length === 1 ? "agente precisa" : "agentes precisam"} revisar o modelo</p><ul className="space-y-1 text-muted-foreground">{affectedAgents.map(reference => <li key={reference.id}><span className="text-foreground">{reference.label}</span>{reference.details[0] && <span> ({reference.details[0]})</span>} · <span className="font-mono">{reference.choice.model}</span> {account.models.some(model => model.id === reference.choice.model) ? "desativado" : "retirado do catálogo"}</li>)}</ul>{onReviewAgents && <Button type="button" variant="outline" size="sm" onClick={() => { setOpen(false); onReviewAgents(affectedAgents.some(reference => reference.kind === "custom_agent") ? "agents" : "flows"); }} className="cursor-pointer">Revisar no Workflow</Button>}</div>}
          {referenceError && <p role="status" className="text-xs text-muted-foreground">Não foi possível verificar os agentes vinculados agora.</p>}
          {(account.providerKind === "openai-codex" || account.custom?.protocol === "openai-responses") && <ProviderTransportSettings key={account.alias} alias={account.alias} />}
        </div>
        <DialogFooter className="shrink-0 flex-row flex-wrap items-center justify-between gap-3 sm:justify-between">
          <label className="flex cursor-pointer items-center gap-2 text-xs">
            <Switch aria-label={`Ativar ${account.alias}`} checked={account.enabled} onCheckedChange={(enabled) => onEnabledChange(account.alias, enabled)} disabled={saving} className="cursor-pointer" />
            <span aria-hidden="true">{account.enabled ? "Ativada" : "Desativada"}</span>
          </label>
          <div className="ml-auto flex items-center gap-2">
          {!custom && onReauthorize && <Button type="button" variant="outline" size="sm" disabled={saving} className="cursor-pointer" onClick={() => { setOpen(false); onReauthorize(account); }}><RefreshCw aria-hidden="true" data-icon="inline-start" />Re-autorizar</Button>}
          {custom && <Button type="button" variant="outline" size="sm" disabled={saving} onClick={() => { setOpen(false); onEdit?.(account); }}><Pencil data-icon="inline-start" />Editar</Button>}
          <Button type="button" variant="destructive" size="sm" disabled={saving} onClick={() => onDisconnect(account.alias)} className="cursor-pointer">
            <Unplug aria-hidden="true" data-icon="inline-start" />
            Desconectar
          </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
