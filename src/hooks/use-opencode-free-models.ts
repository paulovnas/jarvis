import { useEffect, useState } from "react";
import { z } from "zod";
import { readResource, watchResourceRecovery } from "@/core/resource-request";

const freeModelsSchema = z.array(z.object({
  id: z.string().min(1).max(256),
  name: z.string().min(1).max(256),
})).max(100);

/** Mounted only while the Go popover is open. Rust verifies prices and caches
 * the public catalog across accounts; quota failures do not affect this read. */
export function useOpencodeFreeModels() {
  const [models, setModels] = useState<z.infer<typeof freeModelsSchema> | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    let active = true, fetching = false;
    const refresh = async () => {
      if (!active || fetching || document.visibilityState === "hidden") return;
      if (navigator.onLine === false) { setModels(null); setError(true); return; }
      fetching = true;
      try {
        const next = freeModelsSchema.parse(await readResource("get_opencode_go_free_models"));
        if (active) { setModels(next); setError(false); }
      } catch {
        // Never keep a stale "Grátis" label after failing to confirm its price.
        if (active) { setModels(null); setError(true); }
      } finally { fetching = false; }
    };
    void refresh();
    const stopRecovery = watchResourceRecovery(() => { void refresh(); });
    return () => { active = false; stopRecovery(); };
  }, []);
  return { models, error };
}
