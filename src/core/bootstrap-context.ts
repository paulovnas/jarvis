import { createContext } from "react";
import type { BootstrapResources, ProviderUsageCacheEntry } from "@/core/bootstrap";
import type { CoreSnapshot } from "@/core/core-components";
import type { LibrarySnapshot } from "@/core/library";
import type { ProviderAccount } from "@/core/provider-accounts";
import type { SkillSnapshot } from "@/core/skills";

export type BootstrapResourcesController = {
  resources: BootstrapResources;
  updateCore: (snapshot: CoreSnapshot, checked?: boolean) => void;
  updateSkills: (snapshot: SkillSnapshot, checked?: boolean) => void;
  updateAccounts: (accounts: ProviderAccount[]) => void;
  updateUsage: (alias: string, entry: ProviderUsageCacheEntry) => void;
  updateLibrary: (library: LibrarySnapshot) => void;
};

export const BootstrapResourcesContext = createContext<BootstrapResourcesController | null>(null);
