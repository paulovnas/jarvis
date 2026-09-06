import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { accountUsageSchema, type AccountUsage } from "@/core/provider-usage";

export function useProviderUsage(alias: string) {
  const [data, setData] = useState<AccountUsage | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    let active = true, fetching = false;
    const refresh = async () => {
      if (fetching || document.visibilityState === "hidden") return;
      fetching = true;
      try {
        const result = accountUsageSchema.parse(await invoke("get_provider_usage", { alias }));
        if (result.alias !== alias) throw new Error("Account mismatch");
        if (active) { setData(result); setError(false); }
      } catch { if (active) setError(true); }
      finally { fetching = false; }
    };
    void refresh();
    const timer = setInterval(() => { void refresh(); }, 61_000);
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => { active = false; clearInterval(timer); window.removeEventListener("focus", refresh); document.removeEventListener("visibilitychange", refresh); };
  }, [alias]);
  return { data, error };
}
