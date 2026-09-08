import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, CircleAlert, Link2, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { ChoiceField } from "./workflow/WorkflowFields";
import { libraryError } from "@/core/library";
import type { ProviderAccount } from "@/core/provider-accounts";
import { compatibleModels, defaultChoice, modelProblem, providerRemovalPlanSchema, providerRemovalResultSchema, type ModelChoice, type ProviderReference, type ProviderRemovalPlan, type ProviderRemovalResult } from "@/core/provider-references";
import { reasoningLabel } from "@/core/reasoning";

const UNASSIGNED = "__unassigned__";
function ReferenceRow({ item, accounts, choice, busy, onChange }: { item: ProviderReference; accounts: ProviderAccount[]; choice?: ModelChoice; busy: boolean; onChange: (choice?: ModelChoice) => void }) {
  const available = accounts.filter(account => compatibleModels(account, item.kind).length > 0);
  const account = available.find(account => account.alias === choice?.account);
  const models = account ? compatibleModels(account, item.kind) : [];
  const model = models.find(model => model.id === choice?.model);
  const hasReasoning = !["web_search", "vision", "image_generation"].includes(item.kind);
  const selectModel = (next: ModelChoice) => onChange(hasReasoning ? next : { ...next, reasoning: null });
  return <Card className="gap-3 rounded-lg p-4" aria-label={`Vínculo: ${item.label}`}>
    <div className="flex flex-wrap items-center gap-2"><Link2 className="size-3.5 shrink-0 text-onedark-cyan" aria-hidden="true" /><span className="text-sm font-medium">{item.label}</span><Badge variant="outline" className={`rounded-md ${choice ? "border-onedark-green/30 text-onedark-green" : "border-onedark-yellow/30 text-onedark-yellow"}`}>{choice ? "Substituição definida" : "Sem substituição"}</Badge></div>
    <p className="text-xs leading-relaxed text-muted-foreground">{item.details.join(" · ")}</p>
    <div className="grid min-w-0 gap-3 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1.4fr)] sm:items-center">
      <div className="min-w-0 rounded-md border border-border bg-sidebar p-3"><p className="micro-label mb-2 text-muted-foreground">De · Atual</p><p className="break-all font-mono text-xs">{item.choice.account}</p><p className="mt-1 break-all font-mono text-xs text-muted-foreground">{item.choice.model || "Modelo não configurado"}{item.choice.reasoning ? ` · ${item.choice.reasoning}` : ""}</p></div>
      <ArrowRight className="hidden size-4 text-muted-foreground sm:block" aria-hidden="true" />
      <div className="min-w-0 space-y-3"><p className="micro-label text-muted-foreground">Para · Opcional</p>
        <ChoiceField label={`Novo provedor para ${item.label}`} value={choice?.account ?? UNASSIGNED} disabled={busy} options={[{ value: UNASSIGNED, label: "Não substituir agora" }, ...available.map(account => ({ value: account.alias, label: account.alias }))]} onChange={alias => { const next = available.find(account => account.alias === alias); if (next) selectModel(defaultChoice(next, compatibleModels(next, item.kind)[0])); else onChange(undefined); }} />
        {choice && <ChoiceField label={`Novo modelo para ${item.label}`} value={choice.model} disabled={busy || !models.length} options={models.map(model => ({ value: model.id, label: model.name }))} onChange={id => { const next = models.find(model => model.id === id); if (account && next) selectModel(defaultChoice(account, next)); }} />}
        {choice && hasReasoning && !!model?.reasoningLevels.length && <ChoiceField label={`Raciocínio para ${item.label}`} value={choice.reasoning ?? ""} disabled={busy} options={model.reasoningLevels.map(value => ({ value, label: reasoningLabel(value) }))} onChange={reasoning => onChange({ ...choice, reasoning })} />}
        {!available.length && <p className="text-xs leading-relaxed text-onedark-yellow">Nenhum outro provedor compatível. Você pode remover e configurar este item depois.</p>}
        {choice && modelProblem(choice, accounts, item.kind) && <p role="alert" className="text-xs text-destructive">{modelProblem(choice, accounts, item.kind)}</p>}
      </div>
    </div>
  </Card>;
}

export function ProviderRemovalDialog({ alias, accounts, onClose, onRemoved, onBusyChange }: { alias: string; accounts: ProviderAccount[]; onClose: () => void; onRemoved: (result: ProviderRemovalResult) => Promise<void>; onBusyChange: (busy: boolean) => void }) {
  const [plan, setPlan] = useState<ProviderRemovalPlan | null>(null);
  const [choices, setChoices] = useState<Record<string, ModelChoice>>({});
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [needsRefresh, setNeedsRefresh] = useState(false);
  const version = useRef(0); const mounted = useRef(false); const flight = useRef(false);
  const refresh = useCallback(() => {
    const request = ++version.current;
    return invoke("get_provider_removal_plan", { alias }).then(value => {
      const next = providerRemovalPlanSchema.parse(value);
      if (next.alias !== alias) throw new Error("O provedor consultado mudou. Reabra a confirmação.");
      if (mounted.current && request === version.current) { setPlan(next); setChoices(current => Object.fromEntries(Object.entries(current).filter(([id]) => next.items.some(item => item.id === id)))); setNeedsRefresh(false); }
    }).catch(cause => {
      if (mounted.current && request === version.current) { const message = libraryError(cause, "Não foi possível verificar os vínculos deste provedor."); setError(message); setNeedsRefresh(true); toast.error(message); }
    }).finally(() => { if (mounted.current && request === version.current) setLoading(false); });
  }, [alias]);
  useEffect(() => { mounted.current = true; void refresh(); return () => { mounted.current = false; version.current += 1; }; }, [refresh]);
  const available = accounts.filter(account => account.alias !== alias);
  const selected = plan?.items.filter(item => choices[item.id]) ?? [];
  const unresolved = (plan?.items.length ?? 0) - selected.length;
  const invalid = selected.some(item => modelProblem(choices[item.id], available, item.kind));
  async function remove() {
    if (!plan || loading || needsRefresh || flight.current || invalid) return;
    flight.current = true; setBusy(true); onBusyChange(true); setError(null);
    try {
      const result = providerRemovalResultSchema.parse(await invoke("disconnect_provider_account", { alias, revision: plan.revision, replacements: selected.map(item => ({ id: item.id, choice: choices[item.id] })) }));
      await onRemoved(result);
    } catch (cause) {
      const message = libraryError(cause, "Não foi possível remover o provedor. As seleções foram mantidas.");
      if (mounted.current) { setError(message); if (typeof cause === "object" && cause !== null && "code" in cause && cause.code === "provider_links_changed") setNeedsRefresh(true); }
      toast.error(message);
    } finally { flight.current = false; if (mounted.current) setBusy(false); onBusyChange(false); }
  }
  return <Dialog open onOpenChange={open => { if (!open && !flight.current) onClose(); }}><DialogContent className="dark flex max-h-[88dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl" showCloseButton={!busy}>
    <DialogHeader className="shrink-0 border-b border-border p-5 pr-12"><DialogTitle className="flex items-center gap-2"><Trash2 className="size-4 text-destructive" />Remover provedor?</DialogTitle><DialogDescription>O provedor <span className="break-all font-mono text-foreground">{alias}</span> será removido. Revise os itens que usam seus modelos antes de continuar.</DialogDescription></DialogHeader>
    <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-5">
      {loading ? <div role="status" aria-label="Verificando vínculos do provedor" className="space-y-3"><Skeleton className="h-20 w-full" /><Skeleton className="h-48 w-full" /><Skeleton className="h-48 w-full" /></div> : <>
        {plan?.items.length ? <Alert className="border-onedark-yellow/30 bg-onedark-yellow/5"><CircleAlert className="text-onedark-yellow" /><AlertTitle className="text-onedark-yellow">Recomendamos substituir os modelos vinculados</AlertTitle><AlertDescription className="text-xs leading-relaxed text-foreground">A substituição é opcional. Itens sem destino ficarão com uma referência inválida e precisarão ser corrigidos para voltar a funcionar. As escolhas valem para o uso futuro; o histórico das conversas será preservado.</AlertDescription></Alert> : plan && <p className="text-sm text-muted-foreground">Nenhum item configurado está vinculado a este provedor.</p>}
        {plan?.items.map(item => <ReferenceRow key={item.id} item={item} accounts={available} choice={choices[item.id]} busy={busy} onChange={choice => setChoices(current => { const next = { ...current }; if (choice) next[item.id] = choice; else delete next[item.id]; return next; })} />)}
      </>}
      {error && <Alert variant="destructive"><CircleAlert /><AlertTitle>Não foi possível concluir</AlertTitle><AlertDescription className="space-y-2"><p>{error}</p>{needsRefresh && <Button type="button" variant="outline" size="sm" className="cursor-pointer" disabled={busy || loading} onClick={() => { setLoading(true); setError(null); void refresh(); }}>Recarregar vínculos</Button>}</AlertDescription></Alert>}
    </div>
    <DialogFooter className="m-0 shrink-0 flex-col gap-4 border-t border-border bg-card p-5 sm:flex-row sm:items-center sm:justify-between">
      <p role="status" aria-live="polite" className={`min-w-0 text-xs ${unresolved ? "text-onedark-yellow" : "text-muted-foreground"}`}>{loading ? "Verificando vínculos…" : plan?.items.length ? `${selected.length} de ${plan.items.length} substituições definidas${unresolved ? ` · ${unresolved} sem destino` : ""}` : "A conexão e as credenciais serão removidas."}</p>
      <div className="flex shrink-0 justify-end gap-3"><Button type="button" variant="outline" className="cursor-pointer" disabled={busy} onClick={onClose}>Cancelar</Button><Button type="button" variant="destructive" className="cursor-pointer" disabled={busy || loading || !plan || needsRefresh || invalid} onClick={() => void remove()}>{busy ? "Removendo…" : selected.length ? "Substituir e remover" : "Remover provedor"}</Button></div>
    </DialogFooter>
  </DialogContent></Dialog>;
}
