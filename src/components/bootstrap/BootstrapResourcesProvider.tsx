import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { coreSnapshotSchema, type CoreSnapshot } from "@/core/core-components";
import type { LibrarySnapshot } from "@/core/library";
import { accountList, type ProviderAccount } from "@/core/provider-accounts";
import { readResource, watchResourceRecovery } from "@/core/resource-request";
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
  const accountsRevision = useRef(0);

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
    accountsRevision.current += 1;
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

  useEffect(() => {
    let active = true, checkingCore = false, checkingAccounts = false;
    let needsCore = !initial.checked.core, needsAccounts = true;
    const refresh = () => {
      if (!active || navigator.onLine === false) return;
      if (needsCore && !checkingCore) {
        checkingCore = true;
        void readResource("check_core_updates").then(value => {
          const snapshot = coreSnapshotSchema.parse(value);
          if (active) { updateCore(snapshot, !snapshot.checking); needsCore = snapshot.checking; }
        }).catch(() => { /* Preserve installed resources; retry after reconnection. */ })
          .finally(() => { checkingCore = false; });
      }
      if (needsAccounts && !checkingAccounts) {
        checkingAccounts = true;
        const revision = accountsRevision.current;
        void readResource("list_provider_accounts").then(value => {
          const accounts = accountList(value);
          if (active && revision === accountsRevision.current) updateAccounts(accounts);
          needsAccounts = accounts.some(account => account.enabled && account.providerKind !== "custom" && (!account.modelsAvailable || account.modelsStale));
        }).catch(() => { /* Keep the last known catalog usable while offline. */ })
          .finally(() => { checkingAccounts = false; });
      }
    };
    const stop = watchResourceRecovery(refresh);
    refresh();
    return () => { active = false; stop(); };
  }, [initial.checked.core, updateAccounts, updateCore]);

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
