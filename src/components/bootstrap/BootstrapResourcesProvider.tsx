import { useCallback, useMemo, useState, type ReactNode } from "react";
import type { CoreSnapshot } from "@/core/core-components";
import type { LibrarySnapshot } from "@/core/library";
import type { ProviderAccount } from "@/core/provider-accounts";
import type { SkillSnapshot } from "@/core/skills";
import type { BootstrapResources, ProviderUsageCacheEntry } from "@/core/bootstrap";
import { BootstrapResourcesContext } from "@/core/bootstrap-context";

export function BootstrapResourcesProvider({
  initial,
  children,
}: {
  initial: BootstrapResources;
  children: ReactNode;
}) {
  const [resources, setResources] = useState(initial);

  const updateCore = useCallback((snapshot: CoreSnapshot, checked?: boolean) => {
    setResources((current) => ({
      ...current,
      core: snapshot,
      loaded: { ...current.loaded, core: true },
      checked: checked === undefined ? current.checked : { ...current.checked, core: checked },
    }));
  }, []);
  const updateSkills = useCallback((snapshot: SkillSnapshot, checked?: boolean) => {
    setResources((current) => ({
      ...current,
      skills: snapshot,
      loaded: { ...current.loaded, skills: true },
      checked: checked === undefined ? current.checked : { ...current.checked, skills: checked },
    }));
  }, []);
  const updateAccounts = useCallback((accounts: ProviderAccount[]) => {
    setResources((current) => ({
      ...current,
      accounts,
      loaded: { ...current.loaded, accounts: true },
    }));
  }, []);
  const updateUsage = useCallback((alias: string, entry: ProviderUsageCacheEntry) => {
    setResources((current) => ({
      ...current,
      usageByAlias: { ...current.usageByAlias, [alias]: entry },
      loaded: { ...current.loaded, usage: true },
    }));
  }, []);
  const updateLibrary = useCallback((library: LibrarySnapshot) => {
    setResources((current) => ({
      ...current,
      library,
      loaded: { ...current.loaded, library: true },
    }));
  }, []);

  const value = useMemo(() => ({
    resources,
    updateCore,
    updateSkills,
    updateAccounts,
    updateUsage,
    updateLibrary,
  }), [resources, updateCore, updateSkills, updateAccounts, updateUsage, updateLibrary]);

  return <BootstrapResourcesContext.Provider value={value}>{children}</BootstrapResourcesContext.Provider>;
}
