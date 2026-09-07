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

interface Config { accountAlias: string | null; model: string | null; inheritChat: boolean }
const OFF = "off";
const INHERIT = "inherit";
export function WebSearchSettings({ accounts, kind = "web_search", onBusyChange }: { accounts: ProviderAccount[]; kind?: "web_search" | "vision" | "image_generation"; onBusyChange?: (busy: boolean) => void }) {
  const imageGeneration = kind === "image_generation";
  const imageModel = { id: "gemini-3.1-flash-image", name: "Gemini 3.1 Flash Image" };
  const title = imageGeneration ? "Gerar imagens" : kind === "vision" ? "Vision" : "Web Search";
  const [config, setConfig] = useState<Config>({ accountAlias: null, model: null, inheritChat: !imageGeneration });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(false);
  const [retry, setRetry] = useState(0);
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
  const supports = (provider: ProviderAccount, model: string) => provider.providerKind === "custom" ? kind === "vision" && provider.custom?.models.some(item => item.id === model && item.supportsImages) : kind === "vision" ? /^(gpt-|gemini-|claude|o3|o4)/.test(model) : provider.providerKind === "openai-codex" || model.startsWith("gemini-");
  const compatible = accounts.filter(account => account.enabled && (imageGeneration ? account.providerKind === "antigravity" : account.models.some(model => supports(account, model.id))));
  const selected = compatible.find(account => account.alias === config.accountAlias);
  const models = imageGeneration ? [imageModel] : (selected?.models ?? []).filter(model => selected && supports(selected, model.id));
  const unavailable = config.accountAlias !== null && !selected;
  const accountItems = [...(imageGeneration ? [] : [{ value: INHERIT, label: "Herdar do chat" }]), { value: OFF, label: "Desligado" }, ...compatible.map(account => ({ value: account.alias, label: account.alias })), ...(unavailable ? [{ value: config.accountAlias!, label: `${config.accountAlias} · Indisponível` }] : [])];
  const modelItems = models.map(model => ({ value: model.id, label: model.name }));
  if (config.model && !models.some(model => model.id === config.model)) modelItems.push({ value: config.model, label: `${config.model} · Indisponível` });
  async function save(next: Config) {
    if (lock.current || JSON.stringify(next) === JSON.stringify(config)) return;
    lock.current = true; setSaving(true);
    try {
      const saved = webSearchConfigSchema.parse(await invoke(`set_${kind}_config`, { ...next }));
      if (mounted.current) setConfig(saved);
      toast.success(next.inheritChat || next.accountAlias ? `${title} atualizado` : `${title} desligado`);
    } catch { toast.error(`Não foi possível salvar ${title}. A seleção anterior foi mantida.`); }
    finally { lock.current = false; if (mounted.current) setSaving(false); }
  }
  return <Card className="min-w-0 gap-3 py-4">
    <CardHeader className="px-4"><CardTitle className="flex items-center gap-2 text-sm">{imageGeneration ? <ImagePlus className="size-4 text-onedark-green" /> : kind === "vision" ? <Eye className="size-4 text-[#c678dd]" /> : <Globe className="size-4 text-primary" />}{title}</CardTitle></CardHeader>
    <CardContent className="px-4">
      {loading ? <Skeleton className="h-8 w-full" role="status" aria-label={`Carregando ${title}`} /> : error ? <div className="flex gap-2"><p role="alert" className="text-xs text-destructive">Não foi possível carregar {title}.</p><Button variant="outline" size="sm" onClick={() => { setLoading(true); setRetry(value => value + 1); }}>Recarregar {title}</Button></div> : <div className="grid min-w-0 gap-3" aria-busy={saving}>
        <Select items={accountItems} value={config.inheritChat ? INHERIT : config.accountAlias ?? OFF} disabled={saving} onValueChange={value => {
          if (!value) return;
          if (value === OFF || value === INHERIT) { void save({ accountAlias: null, model: null, inheritChat: value === INHERIT }); return; }
          const account = compatible.find(item => item.alias === value);
          const model = imageGeneration ? imageModel : account?.models.find(item => supports(account, item.id));
          if (!model) { toast.error("Nenhum modelo disponível nesta conta."); return; }
          void save({ accountAlias: value, model: model.id, inheritChat: false });
        }}>
          <SelectTrigger className="w-full cursor-pointer text-xs" aria-label={`Provedor de ${title}`}><SelectValue /></SelectTrigger>
          <SelectContent>{accountItems.map(item => <SelectItem key={item.value} value={item.value} disabled={unavailable && item.value === config.accountAlias} className="cursor-pointer">{item.label}</SelectItem>)}</SelectContent>
        </Select>
        <Select items={modelItems} value={imageGeneration ? imageModel.id : config.model} disabled={imageGeneration || saving || config.inheritChat || !selected || !models.length} onValueChange={model => { if (model) void save({ ...config, model }); }}>
          <SelectTrigger className="w-full cursor-pointer text-xs" aria-label={`Modelo de ${title}`}><SelectValue placeholder={config.inheritChat ? "Modelo do chat" : "Modelo"} /></SelectTrigger>
          <SelectContent>{modelItems.map(item => <SelectItem key={item.value} value={item.value} disabled={!models.some(model => model.id === item.value)} className="cursor-pointer">{item.label}</SelectItem>)}</SelectContent>
        </Select>
        {unavailable && <p role="status" className="text-xs text-muted-foreground">Conta indisponível. Escolha outra ou desligue.</p>}
      </div>}
    </CardContent>
  </Card>;
}
