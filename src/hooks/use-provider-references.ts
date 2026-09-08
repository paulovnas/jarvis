import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import type { ProviderAccount } from "@/core/provider-accounts";
import { modelProblem, providerReferencesSchema, type ModelBinding, type ProviderReference } from "@/core/provider-references";
import { libraryError } from "@/core/library";

const emptyBindings: ModelBinding[] = [];
export function useProviderReferences(accounts: ProviderAccount[], ready: boolean) {
  const [data, setData] = useState<{ references: ProviderReference[]; bindings: ModelBinding[]; accounts: ProviderAccount[] } | null>(null);
  const [loading, setLoading] = useState(true);
  const [settledAccounts, setSettledAccounts] = useState<ProviderAccount[] | null>(null);
  const lastNotice = useRef("");
  useEffect(() => {
    if (!ready) return;
    let active = true; let request = 0;
    const disposers: (() => void)[] = [];
    const refresh = async () => {
      const version = ++request; setLoading(true);
      try { const next = providerReferencesSchema.parse(await invoke("get_provider_model_references")); if (active && version === request) setData({ ...next, accounts }); }
      catch (error) { if (active && version === request) toast.error(libraryError(error, "Não foi possível verificar os vínculos dos modelos."), { id: "provider-reference-load" }); }
      finally { if (active && version === request) { setLoading(false); setSettledAccounts(accounts); } }
    };
    void Promise.all(["provider-model-bindings:changed", "agent-models:changed", "workflow-catalog:changed"].map(event => listen(event, () => { if (active) void refresh(); }).then(stop => { if (active) disposers.push(stop); else stop(); }))).then(() => { if (active) void refresh(); }).catch(() => { if (active) void refresh(); });
    return () => { active = false; disposers.forEach(stop => stop()); };
  }, [accounts, ready]);
  useEffect(() => {
    if (!ready || loading || !data || data.accounts !== accounts) return;
    const issues = data.references.flatMap(item => { const problem = modelProblem(item.choice, accounts, item.kind); return problem ? [{ item, problem }] : []; });
    const fingerprint = JSON.stringify(issues.map(({ item, problem }) => [item.id, problem]));
    if (fingerprint === lastNotice.current) return;
    lastNotice.current = fingerprint;
    if (issues.length) toast.error(`${issues.length} ${issues.length === 1 ? "configuração precisa" : "configurações precisam"} de outro provedor ou modelo`, { id: "provider-invalid-models", description: issues.slice(0, 3).map(({ item, problem }) => `${item.label}: ${problem}`).join(" ") + (issues.length > 3 ? ` E mais ${issues.length - 3}.` : ""), duration: 10000 });
    else toast.dismiss("provider-invalid-models");
  }, [accounts, ready, loading, data]);
  return { bindings: data?.bindings ?? emptyBindings, loading: loading || settledAccounts !== accounts };
}

export function useModelProblemNotice(label: string, problem: string | null, key = label) {
  useEffect(() => { if (problem) toast.error(`${label}: modelo indisponível`, { id: `model-problem:${key}`, description: problem }); else toast.dismiss(`model-problem:${key}`); }, [label, problem, key]);
}
