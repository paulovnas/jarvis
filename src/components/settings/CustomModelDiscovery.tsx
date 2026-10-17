import { useEffect, useRef, useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { customCatalogSchema, customModelSchema, type CustomConfig, type CustomCatalogModel, type CustomModel } from "@/core/custom-provider";
import { readResource } from "@/core/resource-request";
import { Choice, Field } from "./CustomProviderFields";

type Lookup = { key: string; loading: boolean; models?: CustomCatalogModel[]; error?: string };
type Correction = Partial<Pick<CustomModel, "contextWindow" | "maxOutputTokens" | "reasoning" | "defaultReasoningLevel">>;
const validContext = (value: number) => customModelSchema.shape.contextWindow.safeParse(value).success;
const validOutput = (value: number) => customModelSchema.shape.maxOutputTokens.safeParse(value).success;

export function CustomModelDiscovery({ config, apiKey, alias, disabled, onPendingChange, onImport }: {
  config: CustomConfig; apiKey: string; alias?: string; disabled: boolean;
  onPendingChange: (pending: boolean) => void; onImport: (models: CustomModel[]) => void;
}) {
  const key = JSON.stringify([config.baseUrl, config.protocol, config.authMode, apiKey, alias]);
  const version = useRef(0);
  const pendingCallback = useRef(onPendingChange);
  useEffect(() => { pendingCallback.current = onPendingChange; }, [onPendingChange]);
  useEffect(() => () => { version.current += 1; pendingCallback.current(false); }, [key]);
  const [lookup, setLookup] = useState<Lookup | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [context, setContext] = useState(0);
  const [output, setOutput] = useState(0);
  const [tools, setTools] = useState(false);
  const [images, setImages] = useState(false);
  const [corrections, setCorrections] = useState<Record<string, Correction>>({});
  const [importError, setImportError] = useState<string | null>(null);
  const active = lookup?.key === key ? lookup : null;
  const existing = new Set(config.models.map(model => model.id));
  const available = active?.models?.filter(model => !existing.has(model.id)) ?? [];
  const chosen = available.filter(model => selected.has(model.id));
  const needsLimits = chosen.some(model => model.contextWindow === null || model.maxOutputTokens === null);
  const needsTools = chosen.some(model => model.supportsTools === null);
  const needsImages = chosen.some(model => model.supportsImages === null);
  const models = chosen.map((model): CustomModel => {
    const correction = corrections[model.id];
    const reasoning = model.reasoningLevels.length ? config.protocol === "anthropic-messages" ? correction?.reasoning ?? "none" : "effort" : "none";
    return {
      id: model.id, name: model.name,
      contextWindow: correction?.contextWindow ?? model.contextWindow ?? context,
      maxOutputTokens: correction?.maxOutputTokens ?? model.maxOutputTokens ?? output,
      supportsTools: model.supportsTools ?? tools, supportsImages: model.supportsImages ?? images,
      reasoning, reasoningLevels: reasoning === "none" ? [] : model.reasoningLevels.map(level => level.value),
      defaultReasoningLevel: reasoning === "none" ? null : correction?.defaultReasoningLevel ?? model.defaultReasoningLevel,
      thinkingBudget: null,
    };
  });
  const limitCorrections = models.filter(model => corrections[model.id]?.contextWindow !== undefined || corrections[model.id]?.maxOutputTokens !== undefined || !validContext(model.contextWindow) || !validOutput(model.maxOutputTokens));
  const reasoningCorrections = chosen.filter(model => model.reasoningLevels.length && (config.protocol === "anthropic-messages" || !model.reasoningLevels.some(level => level.value === model.defaultReasoningLevel)));

  function correct(id: string, correction: Correction) {
    setCorrections(current => ({ ...current, [id]: { ...current[id], ...correction } }));
    setImportError(null);
  }

  async function discover() {
    if (disabled || active?.loading) return;
    const request = ++version.current;
    setLookup({ key, loading: true }); setImportError(null); onPendingChange(true);
    try {
      const result = customCatalogSchema.parse(await readResource("discover_custom_models", {
        target: { baseUrl: config.baseUrl, protocol: config.protocol, authMode: config.authMode }, apiKey: apiKey || null, alias: alias ?? null,
      }));
      if (request !== version.current) return;
      setLookup({ key, loading: false, models: result.models });
      setSelected(new Set(result.models.filter(model => !existing.has(model.id)).map(model => model.id)));
      setCorrections({}); setContext(0); setOutput(0); setTools(false); setImages(false);
    } catch (error) {
      if (request !== version.current) return;
      const message = typeof error === "object" && error !== null && "message" in error && typeof error.message === "string" ? error.message : "Não foi possível consultar o catálogo. Confira a URL e a chave de API.";
      setLookup({ key, loading: false, error: message });
    } finally { if (request === version.current) onPendingChange(false); }
  }

  function importModels() {
    setImportError(null);
    if (!models.length) return;
    if (config.models.filter(model => model.id || model.name || model.contextWindow || model.maxOutputTokens).length + models.length > 100) {
      setImportError("Selecione menos modelos: cada provedor aceita até 100."); return;
    }
    const invalidLimits = models.filter(model => !validContext(model.contextWindow) || !validOutput(model.maxOutputTokens));
    if (invalidLimits.length) {
      setImportError(`Informe contexto de 4.096 a 100.000.000 tokens e saída de 1 a 10.000.000 tokens, em números inteiros. Revise: ${invalidLimits.map(model => model.id).join(", ")}.`); return;
    }
    const unconfirmedReasoning = chosen.filter(model => model.reasoningLevels.length && config.protocol === "anthropic-messages" && !corrections[model.id]?.reasoning);
    if (unconfirmedReasoning.length) {
      setImportError(`Confirme o formato de raciocínio aceito pela API Messages para: ${unconfirmedReasoning.map(model => model.id).join(", ")}.`); return;
    }
    const invalidModels = models.filter(model => !customModelSchema.safeParse(model).success || model.reasoning !== "none" && !model.reasoningLevels.includes(model.defaultReasoningLevel ?? ""));
    if (invalidModels.length) {
      setImportError(`Revise os dados e o raciocínio padrão de: ${invalidModels.map(model => model.id).join(", ")}.`); return;
    }
    onImport(models);
  }

  return <div className="min-w-0 space-y-3 rounded-lg border p-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <p className="text-xs text-muted-foreground">Importe modelos do catálogo da sua API. A consulta usa a chave informada ou a chave salva.</p>
      <Button type="button" variant="outline" size="sm" disabled={disabled || active?.loading || !config.baseUrl || (!apiKey && !alias)} onClick={() => void discover()}><RefreshCw data-icon="inline-start" />{active?.loading ? "Consultando…" : "Descobrir modelos"}</Button>
    </div>
    {active?.loading && <div role="status" aria-label="Consultando catálogo de modelos" className="space-y-2"><Skeleton className="h-5 w-3/4" /><Skeleton className="h-5 w-1/2" /></div>}
    {active?.error && <p role="alert" className="text-xs text-destructive">{active.error}</p>}
    {active?.models && <>
      <p role="status" className="text-xs text-muted-foreground">{active.models.length} modelos no catálogo · {active.models.length - available.length} já cadastrados. Os modelos existentes mantêm suas configurações.</p>
      {available.length > 0 && <>
        <label className="flex cursor-pointer items-center gap-2 text-xs"><Checkbox checked={chosen.length === available.length} indeterminate={chosen.length > 0 && chosen.length < available.length} disabled={disabled} onCheckedChange={checked => setSelected(new Set(checked ? available.map(model => model.id) : []))} />Selecionar todos os novos modelos</label>
        <div className="max-h-48 space-y-2 overflow-y-auto rounded-md border p-2" role="group" aria-label="Modelos disponíveis para importar">{available.map(model => <label key={model.id} className="flex cursor-pointer items-center gap-2 text-xs"><Checkbox checked={selected.has(model.id)} disabled={disabled} onCheckedChange={checked => setSelected(current => { const next = new Set(current); if (checked) next.add(model.id); else next.delete(model.id); return next; })} /><span className="break-all font-mono">{model.id}</span></label>)}</div>
        {(needsLimits || needsTools || needsImages) && <p className="text-xs text-muted-foreground">O catálogo não informa todos os limites e recursos. Os valores comuns só preenchem dados ausentes; ajustes individuais têm prioridade. O ID será enviado exatamente como informado.</p>}
        {needsLimits && <div className="grid gap-3 sm:grid-cols-2"><Field label="Contexto comum (tokens)" type="number" min={4096} max={100_000_000} value={context || ""} disabled={disabled} placeholder="Conforme o provedor" onChange={event => setContext(Number(event.target.value))} /><Field label="Saída máxima comum (tokens)" type="number" min={1} max={10_000_000} value={output || ""} disabled={disabled} placeholder="Conforme o provedor" onChange={event => setOutput(Number(event.target.value))} /></div>}
        {limitCorrections.length > 0 && <div className="max-h-72 space-y-3 overflow-y-auto rounded-md border p-3" role="group" aria-label="Corrigir limites dos modelos"><p className="text-xs text-muted-foreground">Confira os limites ausentes ou inválidos abaixo. Corrija cada modelo ou use os valores comuns para os dados ausentes.</p>{limitCorrections.map(model => <div key={model.id} className="space-y-2"><p className="break-all font-mono text-xs">{model.id}</p><div className="grid gap-3 sm:grid-cols-2"><Field label={`Contexto (tokens) · ${model.id}`} type="number" min={4096} max={100_000_000} value={model.contextWindow} aria-invalid={!validContext(model.contextWindow)} disabled={disabled} placeholder="Conforme o provedor" onChange={event => correct(model.id, { contextWindow: Number(event.target.value) })} /><Field label={`Saída máxima (tokens) · ${model.id}`} type="number" min={1} max={10_000_000} value={model.maxOutputTokens} aria-invalid={!validOutput(model.maxOutputTokens)} disabled={disabled} placeholder="Conforme o provedor" onChange={event => correct(model.id, { maxOutputTokens: Number(event.target.value) })} /></div></div>)}</div>}
        {reasoningCorrections.map(model => <div key={model.id} className="space-y-3 rounded-md border p-3"><p className="break-all font-mono text-xs">{model.id}</p>{config.protocol === "anthropic-messages" && <><p className="text-xs text-muted-foreground">O catálogo informa os esforços, mas não o formato de raciocínio de Messages. Confirme na documentação da API.</p><Choice label={`Formato de raciocínio · ${model.id}`} value={corrections[model.id]?.reasoning ?? null} items={[{ value: "none", label: "Padrão do provedor" }, { value: "adaptive", label: "Thinking adaptativo" }]} disabled={disabled} onChange={reasoning => correct(model.id, { reasoning })} /></>}{(config.protocol !== "anthropic-messages" || corrections[model.id]?.reasoning === "adaptive") && <Choice label={`Raciocínio padrão · ${model.id}`} value={corrections[model.id]?.defaultReasoningLevel ?? model.defaultReasoningLevel} items={model.reasoningLevels.map(level => ({ value: level.value, label: level.display }))} disabled={disabled} onChange={defaultReasoningLevel => correct(model.id, { defaultReasoningLevel })} />}</div>)}
        <div className="flex flex-wrap gap-4">{needsTools && <label className="flex cursor-pointer items-center gap-2 text-xs"><Switch checked={tools} disabled={disabled} onCheckedChange={setTools} className="cursor-pointer" />Ferramentas nos modelos sem informação</label>}{needsImages && <label className="flex cursor-pointer items-center gap-2 text-xs"><Switch checked={images} disabled={disabled} onCheckedChange={setImages} className="cursor-pointer" />Imagens nos modelos sem informação</label>}</div>
        <Button type="button" variant="secondary" size="sm" disabled={disabled || !chosen.length} onClick={importModels}><Download data-icon="inline-start" />Importar modelos selecionados ({chosen.length})</Button>
      </>}
    </>}
    {importError && <p role="alert" className="text-xs text-destructive">{importError}</p>}
  </div>;
}
