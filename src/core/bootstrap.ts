import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { coreSnapshotSchema, type CoreSnapshot } from "@/core/core-components";
import { readLibrarySnapshot, type LibrarySnapshot } from "@/core/library";
import { accountList, type ProviderAccount } from "@/core/provider-accounts";
import { accountUsageSchema, type AccountUsage } from "@/core/provider-usage";
import { skillsSnapshotSchema, type SkillSnapshot } from "@/core/skills";
import type { BootstrapProgressEvent, BootstrapStepId, BootstrapStepStatus } from "@/core/bootstrap-state";

const appConfigSchema = z.object({ onboardingCompleted: z.boolean() });

export type AppConfig = z.infer<typeof appConfigSchema>;
export type ProviderUsageCacheEntry = {
  data: AccountUsage | null;
  error: boolean;
};

export type BootstrapResources = {
  core: CoreSnapshot | null;
  skills: SkillSnapshot | null;
  accounts: ProviderAccount[];
  usageByAlias: Record<string, ProviderUsageCacheEntry>;
  library: LibrarySnapshot | null;
  checked: {
    core: boolean;
    skills: boolean;
  };
  loaded: {
    core: boolean;
    skills: boolean;
    accounts: boolean;
    usage: boolean;
    library: boolean;
  };
  warnings: string[];
};

export type AppBootstrapResult = {
  config: AppConfig;
  resources: BootstrapResources | null;
};

function report(
  callback: (event: BootstrapProgressEvent) => void,
  id: BootstrapStepId,
  progress: number,
  status: BootstrapStepStatus,
  detail: string,
) {
  callback({ id, progress: Math.max(0, Math.min(1, progress)), status, detail });
}

async function loadCore(
  onProgress: (event: BootstrapProgressEvent) => void,
  warnings: string[],
): Promise<{ snapshot: CoreSnapshot | null; checked: boolean }> {
  report(onProgress, "core", 0.12, "running", "Validando instalações e versões");
  try {
    const snapshot = coreSnapshotSchema.parse(await invoke("check_core_updates"));
    report(onProgress, "core", 1, "complete", snapshot.ready ? "Core pronto" : "Core requer atenção");
    return { snapshot, checked: true };
  } catch {
    warnings.push("Não foi possível verificar as atualizações do Core.");
    try {
      const snapshot = coreSnapshotSchema.parse(await invoke("get_core_status"));
      report(onProgress, "core", 1, "warning", "Estado local carregado; atualização indisponível");
      return { snapshot, checked: false };
    } catch {
      report(onProgress, "core", 1, "warning", "Core indisponível neste momento");
      return { snapshot: null, checked: false };
    }
  }
}

async function loadProviders(
  onProgress: (event: BootstrapProgressEvent) => void,
  warnings: string[],
): Promise<{
  accounts: ProviderAccount[];
  usageByAlias: Record<string, ProviderUsageCacheEntry>;
  accountsLoaded: boolean;
  usageLoaded: boolean;
}> {
  report(onProgress, "providers", 0.08, "running", "Carregando contas conectadas");
  let accounts: ProviderAccount[];
  try {
    accounts = accountList(await invoke("list_provider_accounts"));
  } catch {
    warnings.push("Não foi possível carregar os provedores conectados.");
    report(onProgress, "providers", 1, "warning", "Provedores indisponíveis neste momento");
    return { accounts: [], usageByAlias: {}, accountsLoaded: false, usageLoaded: false };
  }

  const eligible = accounts.filter(
    (account) => account.enabled && account.providerKind !== "custom",
  );
  if (eligible.length === 0) {
    report(onProgress, "providers", 1, "complete", accounts.length ? "Provedores carregados" : "Nenhum provedor conectado");
    return { accounts, usageByAlias: {}, accountsLoaded: true, usageLoaded: true };
  }

  report(onProgress, "providers", 0.34, "running", "Consultando limites de uso");
  const usageByAlias: Record<string, ProviderUsageCacheEntry> = {};
  let completed = 0;
  await Promise.all(eligible.map(async (account) => {
    try {
      const parsed = accountUsageSchema.parse(await invoke("get_provider_usage", { alias: account.alias }));
      usageByAlias[account.alias] = parsed.alias === account.alias
        ? { data: parsed, error: false }
        : { data: null, error: true };
    } catch {
      usageByAlias[account.alias] = { data: null, error: true };
    } finally {
      completed += 1;
      report(
        onProgress,
        "providers",
        0.34 + 0.66 * completed / eligible.length,
        "running",
        `Limites consultados · ${completed}/${eligible.length}`,
      );
    }
  }));

  const failures = Object.values(usageByAlias).filter((entry) => entry.error).length;
  if (failures > 0) warnings.push(`Os limites de ${failures} ${failures === 1 ? "provedor" : "provedores"} não puderam ser atualizados.`);
  report(
    onProgress,
    "providers",
    1,
    failures > 0 ? "warning" : "complete",
    failures > 0 ? "Contas prontas; alguns limites estão indisponíveis" : "Provedores e limites prontos",
  );
  return { accounts, usageByAlias, accountsLoaded: true, usageLoaded: true };
}

async function loadSkills(
  onProgress: (event: BootstrapProgressEvent) => void,
  warnings: string[],
): Promise<{ snapshot: SkillSnapshot | null; checked: boolean }> {
  report(onProgress, "skills", 0.1, "running", "Lendo catálogo instalado");
  let snapshot: SkillSnapshot;
  try {
    snapshot = skillsSnapshotSchema.parse(await invoke("list_skills"));
  } catch {
    warnings.push("Não foi possível carregar as skills instaladas.");
    report(onProgress, "skills", 1, "warning", "Catálogo de skills indisponível");
    return { snapshot: null, checked: false };
  }

  // Marketplace repositories can be large and slow. Startup only materializes
  // the local catalog; update discovery remains available on demand in Skills.
  report(onProgress, "skills", 1, "complete", snapshot.skills.length ? "Skills instaladas carregadas" : "Nenhuma skill instalada");
  return { snapshot, checked: false };
}

async function loadWorkspace(
  onProgress: (event: BootstrapProgressEvent) => void,
  warnings: string[],
): Promise<LibrarySnapshot | null> {
  report(onProgress, "workspace", 0.18, "running", "Restaurando o último contexto");
  try {
    const library = readLibrarySnapshot(await invoke("get_library_snapshot"));
    report(onProgress, "workspace", 1, "complete", "Workspaces e conversas prontos");
    return library;
  } catch {
    warnings.push("Não foi possível pré-carregar os workspaces.");
    report(onProgress, "workspace", 1, "warning", "Workspaces serão carregados novamente");
    return null;
  }
}

export async function loadAppBootstrap(
  onProgress: (event: BootstrapProgressEvent) => void,
  knownConfig?: AppConfig,
): Promise<AppBootstrapResult> {
  report(onProgress, "configuration", 0.15, "running", "Lendo preferências do Jarvis");
  const config = knownConfig ?? appConfigSchema.parse(await invoke("get_app_config"));
  report(onProgress, "configuration", 1, "complete", "Configuração carregada");

  if (!config.onboardingCompleted) return { config, resources: null };

  const warnings: string[] = [];
  const [core, providers, skills, library] = await Promise.all([
    loadCore(onProgress, warnings),
    loadProviders(onProgress, warnings),
    loadSkills(onProgress, warnings),
    loadWorkspace(onProgress, warnings),
  ]);

  return {
    config,
    resources: {
      core: core.snapshot,
      skills: skills.snapshot,
      accounts: providers.accounts,
      usageByAlias: providers.usageByAlias,
      library,
      checked: { core: core.checked, skills: skills.checked },
      loaded: {
        core: core.snapshot !== null,
        skills: skills.snapshot !== null,
        accounts: providers.accountsLoaded,
        usage: providers.usageLoaded,
        library: library !== null,
      },
      warnings,
    },
  };
}
