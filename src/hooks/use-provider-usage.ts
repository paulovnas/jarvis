import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useBootstrapResources } from "@/hooks/use-bootstrap-resources";
import { accountUsageSchema, type AccountUsage } from "@/core/provider-usage";

export function useProviderUsage(alias: string, { pollWhileHidden = false }: { pollWhileHidden?: boolean } = {}) {
  const bootstrap = useBootstrapResources();
  const updateBootstrapUsage = bootstrap?.updateUsage;
  const cached = bootstrap?.resources.usageByAlias[alias];
  const [localData, setLocalData] = useState<AccountUsage | null>(() => cached?.data ?? null);
  const [localError, setLocalError] = useState(() => cached?.error ?? false);
  const data = cached ? cached.data : localData;
  const error = cached ? cached.error : localError;
  const initialCached = useRef(cached);
  const latestData = useRef(data);
  useEffect(() => { latestData.current = data; }, [data]);
  useEffect(() => {
    let active = true, fetching = false;
    const cachedAtStart = initialCached.current;
    const cachedIsFresh = cachedAtStart?.data?.fetchedAt != null && Date.now() - cachedAtStart.data.fetchedAt < 60_000;
    const store = (next: AccountUsage | null, failed: boolean) => {
      if (updateBootstrapUsage) updateBootstrapUsage(alias, { data: next, error: failed });
      else { setLocalData(next); setLocalError(failed); }
    };
    const refresh = async () => {
      if (fetching || (!pollWhileHidden && document.visibilityState === "hidden")) return;
      fetching = true;
      try {
        const result = accountUsageSchema.parse(await invoke("get_provider_usage", { alias }));
        if (result.alias !== alias) throw new Error("Account mismatch");
        if (active) store(result, false);
      } catch { if (active) store(latestData.current, true); }
      finally { fetching = false; }
    };
    if (!cachedIsFresh) void refresh();
    const timer = setInterval(() => { void refresh(); }, 61_000);
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => { active = false; clearInterval(timer); window.removeEventListener("focus", refresh); document.removeEventListener("visibilitychange", refresh); };
  }, [alias, pollWhileHidden, updateBootstrapUsage]);
  return { data, error };
}
