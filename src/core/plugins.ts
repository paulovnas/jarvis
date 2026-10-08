import { z } from "zod";

export const pluginComponentSchema = z.object({
  id: z.string(), name: z.string(), kind: z.enum(["skills", "mcp", "hooks", "apps"]),
  enabled: z.boolean(), trusted: z.boolean(), supported: z.boolean(), detail: z.string(),
  mcpServerId: z.string().nullable().optional(), mcpOAuth: z.boolean().optional(),
  appConnectUrl: z.string().url().refine(value => { try { const url = new URL(value); return url.origin === "https://chatgpt.com" && !url.username && !url.password; } catch { return false; } }).nullable().optional(),
});
export type PluginComponent = z.infer<typeof pluginComponentSchema>;
export const PLUGIN_COMPONENT_LABELS: Record<PluginComponent["kind"], string> = { skills: "Skills", mcp: "MCPs", hooks: "Hooks", apps: "Apps" };
export const pluginMarketplaceSchema = z.object({ id: z.string(), name: z.string(), source: z.string(), refName: z.string().nullable(), sparsePaths: z.array(z.string()), refreshed: z.boolean(), builtIn: z.boolean() });
export type PluginMarketplace = z.infer<typeof pluginMarketplaceSchema>;
export const pluginPackageSourceSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("local"), path: z.string() }),
  z.object({ kind: z.literal("git"), url: z.string(), path: z.string().nullable(), refName: z.string().nullable(), sha: z.string().nullable() }),
  z.object({ kind: z.literal("npm"), package: z.string(), version: z.string().nullable(), registry: z.string().nullable() }),
]);
const pluginPresentation = {
  category: z.string().nullable().optional(),
  shortDescription: z.string().nullable().optional(),
  iconDataUrl: z.string().max(1_500_000).regex(/^data:image\/(?:png|jpeg|gif|webp|svg\+xml);base64,[A-Za-z0-9+/]+={0,2}$/).nullable().optional(),
};
export const availablePluginSchema = z.object({
  id: z.string(), name: z.string(), marketplaceId: z.string(), displayName: z.string(), description: z.string(),
  version: z.string().nullable(), source: pluginPackageSourceSchema, installable: z.boolean(), authentication: z.string(), requirements: z.array(z.string()),
  ...pluginPresentation,
});
export type AvailablePlugin = z.infer<typeof availablePluginSchema>;
export const installedPluginSchema = z.object({
  id: z.string(), name: z.string(), marketplaceId: z.string(), displayName: z.string(), description: z.string(), version: z.string(), hash: z.string(),
  rootPath: z.string(), dataPath: z.string(), enabled: z.boolean(), integrityValid: z.boolean(), components: z.array(pluginComponentSchema),
  projectOverrides: z.record(z.string(), z.boolean()), warnings: z.array(z.string()),
  ...pluginPresentation,
});
export type InstalledPlugin = z.infer<typeof installedPluginSchema>;
export const pluginCatalogSchema = z.object({ revision: z.number().int().nonnegative(), marketplaces: z.array(pluginMarketplaceSchema), available: z.array(availablePluginSchema), installed: z.array(installedPluginSchema), issues: z.array(z.string()), appsAccountId: z.string().nullable() });
export type PluginCatalog = z.infer<typeof pluginCatalogSchema>;
export const pluginPreviewSchema = z.object({ title: z.string(), description: z.string(), source: z.string(), hash: z.string(), components: z.array(pluginComponentSchema), commands: z.array(z.string()), requirements: z.array(z.string()), warnings: z.array(z.string()), affectedIds: z.array(z.string()) });
export type PluginPreview = z.infer<typeof pluginPreviewSchema>;
export const pluginReceiptSchema = z.object({ receiptId: z.string(), revision: z.number().int().nonnegative(), preview: pluginPreviewSchema });
export type PluginReceipt = z.infer<typeof pluginReceiptSchema>;
const nonempty = z.string().min(1);
export const pluginDraftSchema = z.object({ name: z.string().regex(/^[A-Za-z0-9._-]{1,64}$/), description: z.string(), skills: z.array(z.object({ name: nonempty, content: nonempty }).strict()).max(64), mcpServers: z.record(z.string(), z.unknown()), hooks: z.unknown().nullable(), apps: z.record(z.string(), z.unknown()), files: z.array(z.object({ path: nonempty, content: z.string() }).strict()).max(128).default([]) }).strict();
export type PluginDraft = z.infer<typeof pluginDraftSchema>;
export const pluginOperationSchema = z.discriminatedUnion("action", [
  z.object({ action: z.literal("addMarketplace"), source: nonempty, refName: z.string().nullable().optional(), sparsePaths: z.array(z.string()).optional() }).strict(),
  z.object({ action: z.literal("refreshMarketplace"), marketplaceId: z.string().nullable().optional() }).strict(),
  z.object({ action: z.literal("removeMarketplace"), marketplaceId: nonempty }).strict(),
  z.object({ action: z.literal("install"), pluginId: nonempty }).strict(),
  z.object({ action: z.literal("update"), pluginId: nonempty }).strict(),
  z.object({ action: z.literal("uninstall"), pluginId: nonempty }).strict(),
  z.object({ action: z.literal("setEnabled"), pluginId: nonempty, enabled: z.boolean(), projectPath: z.string().nullable().optional() }).strict(),
  z.object({ action: z.literal("configureComponent"), pluginId: nonempty, componentId: nonempty, enabled: z.boolean() }).strict(),
  z.object({ action: z.literal("trustHooks"), pluginId: nonempty, trusted: z.boolean() }).strict(),
  z.object({ action: z.literal("setAppsAccount"), accountId: z.string().nullable() }).strict(),
  z.object({ action: z.literal("import"), path: nonempty }).strict(),
  z.object({ action: z.literal("create"), draft: pluginDraftSchema }).strict(),
]);
export type PluginOperation = z.infer<typeof pluginOperationSchema>;

export function pluginError(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error && typeof error.message === "string") return error.message;
  return "Não foi possível atualizar os plugins. Tente novamente.";
}

export function pluginSourceLabel(source: AvailablePlugin["source"]): string {
  if (source.kind === "local") return source.path;
  if (source.kind === "npm") return `${source.package}${source.version ? `@${source.version}` : ""}`;
  return `${source.url}${source.path ? ` · ${source.path}` : ""}${source.refName ? ` · ${source.refName}` : ""}`;
}

const CATEGORY_LABELS: Record<string, string> = {
  productivity: "Produtividade", communication: "Comunicação", creativity: "Criatividade", finance: "Finanças",
  "developer tools": "Ferramentas de desenvolvimento", "education & research": "Educação e pesquisa",
  security: "Segurança", "data & analytics": "Dados e análise", "business & operations": "Negócios e operações",
  "scientific research": "Pesquisa científica", design: "Design",
  development: "Desenvolvimento", database: "Bancos de dados", monitoring: "Monitoramento",
  deployment: "Implantação", automation: "Automação", learning: "Aprendizado", location: "Localização",
  testing: "Testes", math: "Matemática", migration: "Migrações", documentation: "Documentação",
};

export function pluginCategoryLabel(category: string | null | undefined): string {
  const value = category?.trim();
  return value ? CATEGORY_LABELS[value.toLocaleLowerCase("pt-BR")] ?? value : "Mais plugins";
}

export function groupPlugins<T extends AvailablePlugin | InstalledPlugin>(plugins: T[]): { category: string; title: string; plugins: T[] }[] {
  const groups = new Map<string, { category: string; title: string; plugins: T[] }>();
  for (const plugin of plugins) {
    const category = plugin.category?.trim().toLocaleLowerCase("pt-BR") ?? "";
    let group = groups.get(category);
    if (!group) { group = { category, title: pluginCategoryLabel(plugin.category), plugins: [] }; groups.set(category, group); }
    group.plugins.push(plugin);
  }
  return [...groups.values()];
}

export function pluginMonogram(name: string): string {
  const words = name.match(/[\p{L}\p{N}]+/gu) ?? [];
  const [first = "", second] = words;
  return (second ? `${first[0] ?? ""}${second[0] ?? ""}` : (first.slice(0, 2) || "PL")).toLocaleUpperCase("pt-BR");
}
