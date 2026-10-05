import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { accountList, type ProviderAccount } from "@/core/provider-accounts";
import { PROVIDER_SETTINGS_CHANGED } from "@/core/auxiliary-windows";
import { readResource } from "@/core/resource-request";

/** Signals carry no account data; every window reads the authoritative native state. */
export function useProviderSettingsChanges(onChange: (accounts: ProviderAccount[]) => void) {
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let active = true;
    let revision = 0;
    let dispose: (() => void) | undefined;
    const refresh = () => {
      const request = ++revision;
      void readResource("list_provider_accounts", { cached: true }).then(result => {
        if (active && request === revision) onChange(accountList(result));
      }).catch(() => {});
    };
    void listen(PROVIDER_SETTINGS_CHANGED, refresh).then(unlisten => { if (active) dispose = unlisten; else unlisten(); }).catch(() => {});
    return () => { active = false; dispose?.(); };
  }, [onChange]);
}
