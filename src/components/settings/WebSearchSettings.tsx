import { providerDisplayName } from "@/core/provider-accounts";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Eye, Globe, ImagePlus } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import type { ProviderAccount } from "@/core/provider-accounts";
import { webSearchConfigSchema } from "@/core/web-search";
import { compatibleModels, modelProblem } from "@/core/provider-references";
import { useModelProblemNotice } from "@/hooks/use-provider-references";
import { museModels } from "@/core/executors";
import { useMuseRuntime } from "@/hooks/use-muse-runtime";
import { supportsModelLookup } from "@/core/custom-provider";
import type { ProviderModel } from "@/core/provider-accounts";

interface Config { executor: "jarvis" | "muse"; accountAlias: string | null; model: string | null; inheritChat: boolean }
const OFF = "off";
const INHERIT = "inherit";
const MUSE = "executor:muse";
export function WebSearchSettings({ accounts, kind = "web_search", onBusyChange, onImageCatalogRefresh }: { accounts: ProviderAccount[]; kind?: "web_search" | "vision" | "image_generation"; onBusyChange?: (busy: boolean) => void; onImageCatalogRefresh?: () => Promise<void> }) {
  const imageGeneration = kind === "image_generation";
  const title = imageGeneration ? "Gerar imagens" : kind === "vision" ? "Vision" : "Web Search";
  const [config, setConfig] = useState<Config>({ executor: "jarvis", accountAlias: null, model: null, inheritChat: !imageGeneration });
  const muse = useMuseRuntime(kind === "web_search");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(false);
  const [retry, setRetry] = useState(0);
  const [catalogSettledAttempt, setCatalogSettledAttempt] = useState<number | null>(null);
  const [catalogError, setCatalogError] = useState(false);
  const hasOpenRouter = imageGeneration && accounts.some(account => account.enabled && (account.providerKind === "custom" || account.providerKind === "openrouter") && account.custom && supportsModelLookup(account.custom.baseUrl));
  const catalogLoading = hasOpenRouter && catalogSettledAttempt !== retry;
  const lock = useRef(false);
  const mounted = useRef(false);
  useEffect(() => { onBusyChange?.(loading || saving || error); }, [loading, saving, error, onBusyChange]);
  useEffect(() => {
    let active = true; mounted.current = true;
    void invoke(`get_${kind}_config`).then(value => {
      if (active) { setConfig(webSearchConfigSchema.parse(value)); setError(false); }
    }).catch(() => { if (active) setError(true); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; mounted.current = false; };
  }, [kind, retry]);
  useEffect(() => {
    if (!hasOpenRouter) return;
    let active = true;
    void invoke<ProviderModel[]>("refresh_image_generation_models").then(models => {
      if (!Array.isArray(models)) throw new Error("Invalid image catalog");
      if (active) { setCatalogError(false); return onImageCatalogRefresh?.(); }
    }).catch(() => { if (active) setCatalogError(true); }).finally(() => { if (active) setCatalogSettledAttempt(retry); });
    return () => { active = false; };
  }, [hasOpenRouter, retry, onImageCatalogRefresh]);
  const availableAccounts = accounts;
  const compatible = availableAccounts.filter(account => compatibleModels(account, kind).length > 0);
  const selected = compatible.find(account => account.alias === config.accountAlias);
  const museSelected = kind === "web_search" && config.executor === "muse" && !config.inheritChat;
  const museCatalog = museModels(muse.data).map(model => ({ id: model.value, name: model.label }));
  const museAvailable = Boolean(muse.data?.installed && muse.data.authenticated && museCatalog.length && !muse.error && !muse.data.error);
  const models = museSelected ? museCatalog : selected ? compatibleModels(selected, kind) : [];
  const unavailable = !museSelected && config.accountAlias !== null && !selected;
  const museProblem = muse.error ?? (muse.data?.preferences?.enabled === false ? "Ative o Muse Code em Configurações → Provedores." : !muse.data?.installed ? "Instale o Muse Code em Configurações → Provedores." : !muse.data.authenticated ? "Conecte sua assinatura Muse Code em Configurações → Provedores." : muse.data.error ?? (!museCatalog.some(model => model.id === config.model) ? "O modelo de busca do Muse Code está indisponível. Selecione outro modelo." : null));
  const problem = loading || error || config.inheritChat || (imageGeneration && catalogLoading) ? null : museSelected ? muse.loading ? null : museProblem : config.accountAlias ? modelProblem({ account: config.accountAlias, model: config.model ?? "", reasoning: null }, availableAccounts, kind) : null;
  useModelProblemNotice(title, problem);
  const accountItems = [...(imageGeneration ? [] : [{ value: INHERIT, label: "Herdar do chat" }]), { value: OFF, label: "Desligado" }, ...(kind === "web_search" ? [{ value: MUSE, label: "Muse Code" }] : []), ...compatible.map(account => ({ value: account.alias, label: providerDisplayName(account) })), ...(unavailable ? [{ value: config.accountAlias!, label: `${config.accountAlias} · Indisponível` }] : [])];
  const modelItems = models.map(model => ({ value: model.id, label: model.name }));
  if (config.model && !models.some(model => model.id === config.model)) modelItems.push({ value: config.model, label: `${config.model} · Indisponível` });
  async function save(next: Config) {
    if (lock.current || JSON.stringify(next) === JSON.stringify(config)) return;
    lock.current = true; setSaving(true);
    try {
      const { executor, ...native } = next;
      const saved = webSearchConfigSchema.parse(await invoke(`set_${kind}_config`, kind === "web_search" && executor === "muse" ? next : native));
      if (mounted.current) setConfig(saved);
      toast.success(next.executor === "muse" || next.inheritChat || next.accountAlias ? `${title} atualizado` : `${title} desligado`);
    } catch { toast.error(`Não foi possível salvar ${title}. A seleção anterior foi mantida.`); }
    finally { lock.current = false; if (mounted.current) setSaving(false); }
  }
  return <Card className="min-w-0 gap-3 py-4">
    <CardHeader className="px-4"><CardTitle className="flex items-center gap-2 text-sm">{imageGeneration ? <ImagePlus className="size-4 text-onedark-green" /> : kind === "vision" ? <Eye className="size-4 text-onedark-purple" /> : <Globe className="size-4 text-primary" />}{title}</CardTitle></CardHeader>
    <CardContent className="px-4">
      {loading ? <Skeleton className="h-8 w-full" role="status" aria-label={`Carregando ${title}`} /> : error ? <div className="flex gap-2"><p role="alert" className="text-xs text-destructive">Não foi possível carregar {title}.</p><Button variant="outline" size="sm" onClick={() => { setLoading(true); setRetry(value => value + 1); }}>Recarregar {title}</Button></div> : <div className="grid min-w-0 gap-3" aria-busy={saving}>
        <Select items={accountItems} value={config.inheritChat ? INHERIT : museSelected ? MUSE : config.accountAlias ?? OFF} disabled={saving} onValueChange={value => {
          if (!value) return;
          if (value === OFF || value === INHERIT) { void save({ executor: "jarvis", accountAlias: null, model: null, inheritChat: value === INHERIT }); return; }
          if (value === MUSE) { if (museAvailable) void save({ executor: "muse", accountAlias: null, model: museCatalog[0].id, inheritChat: false }); return; }
          const account = compatible.find(item => item.alias === value);
          const model = account && compatibleModels(account, kind)[0];
          if (!model) { toast.error("Nenhum modelo disponível nesta conta."); return; }
          void save({ executor: "jarvis", accountAlias: value, model: model.id, inheritChat: false });
        }}>
          <SelectTrigger className="w-full cursor-pointer text-xs" aria-label={`Provedor de ${title}`}><SelectValue /></SelectTrigger>
          <SelectContent>{accountItems.map(item => <SelectItem key={item.value} value={item.value} disabled={item.value === MUSE ? !museAvailable : unavailable && item.value === config.accountAlias} className="cursor-pointer">{item.label}</SelectItem>)}</SelectContent>
        </Select>
        <Select items={modelItems} value={config.model} disabled={saving || config.inheritChat || (museSelected ? !museAvailable : !selected) || !models.length} onValueChange={model => { if (model) void save({ ...config, model }); }}>
          <SelectTrigger className="w-full cursor-pointer text-xs" aria-label={`Modelo de ${title}`}><SelectValue placeholder={config.inheritChat ? "Modelo do chat" : "Modelo"} /></SelectTrigger>
          <SelectContent>{modelItems.map(item => <SelectItem key={item.value} value={item.value} disabled={!models.some(model => model.id === item.value)} className="cursor-pointer">{item.label}</SelectItem>)}</SelectContent>
        </Select>
        {problem && <p role="alert" className="text-xs text-destructive">{problem}</p>}
        {museSelected && <p className="text-xs text-muted-foreground">A busca usa sua assinatura Muse Code, independente do modelo do chat.</p>}
        {imageGeneration && selected?.providerKind === "openai-codex" && <p className="text-xs text-muted-foreground">Usa sua assinatura ChatGPT conectada ao Codex, sem chave de API. A disponibilidade depende da conta e dos limites do plano.</p>}
        {imageGeneration && (selected?.providerKind === "custom" || selected?.providerKind === "openrouter") && <p className="text-xs text-muted-foreground">A geração usa a conta OpenRouter selecionada e consome o saldo de API, independente do modelo do chat.</p>}
        {hasOpenRouter && <><p className="text-xs text-muted-foreground">Para usar outro modelo OpenRouter, adicione-o à conta e habilite Geração de imagens no cadastro do modelo.</p><div className="flex items-center gap-2"><Button variant="outline" size="sm" disabled={catalogLoading || saving} onClick={() => setRetry(value => value + 1)}>{catalogLoading ? "Atualizando modelos…" : "Atualizar modelos de imagens"}</Button>{catalogError && <p role="alert" className="text-xs text-destructive">Não foi possível atualizar o OpenRouter. O catálogo anterior foi mantido.</p>}</div></>}
      </div>}
    </CardContent>
  </Card>;
}
