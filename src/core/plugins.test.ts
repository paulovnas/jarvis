import { describe, expect, it } from "vitest";
import { availablePluginSchema, groupPlugins, pluginCatalogSchema, pluginCategoryLabel, pluginComponentSchema, pluginMonogram, pluginOperationSchema, pluginPreviewSchema, pluginSourceLabel } from "./plugins";

describe("plugin wire contracts", () => {
  it("accepts native catalog and exact verified command previews", () => {
    expect(pluginCatalogSchema.parse({ revision: 4, marketplaces: [], available: [], installed: [], issues: ["Origem indisponível"], appsAccountId: null }).revision).toBe(4);
    const command = '  node "${PLUGIN_ROOT}/hooks/start.mjs"\n';
    expect(pluginPreviewSchema.parse({ title: "Autorizar", description: "", source: "/plugins/example", hash: "digest", components: [], commands: [command], requirements: ["Node.js"], warnings: [], affectedIds: [] }).commands).toEqual([command]);
  });
  it("keeps mutations explicit and rejects accidental command execution fields", () => {
    expect(pluginOperationSchema.parse({ action: "configureComponent", pluginId: "p", componentId: "hooks", enabled: false })).toEqual({ action: "configureComponent", pluginId: "p", componentId: "hooks", enabled: false });
    expect(pluginOperationSchema.safeParse({ action: "install", pluginId: "p", command: "bash install.sh" }).success).toBe(false);
    expect(pluginOperationSchema.safeParse({ action: "uninstall", pluginId: "" }).success).toBe(false);
    expect(pluginOperationSchema.safeParse({ action: "setAppsAccount", accountId: null }).success).toBe(true);
  });
  it("preserves literal managed package files and hook definitions", () => {
    const parsed = pluginOperationSchema.parse({ action: "create", draft: { name: "my-plugin", description: "Description", skills: [{ name: "my-skill", content: "---\nname: my-skill\n---\nLiteral $command" }], mcpServers: { search: { command: "node", args: ["./server.mjs"] } }, hooks: { hooks: { SessionStart: [] } }, apps: {} } });
    expect(parsed.action).toBe("create");
    if (parsed.action === "create") expect(parsed.draft.skills[0].content).toContain("Literal $command");
  });
  it("labels remote/local package origins without claiming popularity", () => {
    expect(pluginSourceLabel({ kind: "git", url: "https://github.com/org/repo.git", path: "plugins/a", refName: "v1", sha: null })).toBe("https://github.com/org/repo.git · plugins/a · v1");
    expect(pluginSourceLabel({ kind: "local", path: "/plugins/a" })).toBe("/plugins/a");
    expect(pluginSourceLabel({ kind: "npm", package: "@org/plugin", version: "1.0", registry: null })).toBe("@org/plugin@1.0");
  });
  it("keeps app authorization on the fixed first-party ChatGPT origin", () => {
    const component = { id: "apps:drive", name: "Drive", kind: "apps", enabled: true, trusted: true, supported: true, detail: "Drive app", mcpServerId: "plugin-app:gateway" };
    expect(pluginComponentSchema.parse({ ...component, appConnectUrl: "https://chatgpt.com/apps/drive/connector-one" }).appConnectUrl).toBe("https://chatgpt.com/apps/drive/connector-one");
    for (const appConnectUrl of ["invalid", "https://example.com/authorize", "javascript:alert(1)", "https://chatgpt.com.evil.test/apps", "https://chatgpt.com:4444/apps", "https://user@chatgpt.com/apps"]) expect(pluginComponentSchema.safeParse({ ...component, appConnectUrl }).success).toBe(false);
  });
  it("preserves real categories and embedded icons without inventing store rankings", () => {
    const packageData = { id: "store:plugin", name: "plugin", marketplaceId: "store", displayName: "Plugin", description: "Full description", version: null, source: { kind: "local", path: "/plugin" }, installable: true, authentication: "ON_INSTALL", requirements: [] };
    const productivity = availablePluginSchema.parse({ ...packageData, category: "Productivity", shortDescription: "Quick summary", iconDataUrl: "data:image/png;base64,aWNvbg==" });
    expect(productivity).toMatchObject({ category: "Productivity", shortDescription: "Quick summary", iconDataUrl: "data:image/png;base64,aWNvbg==" });
    const education = availablePluginSchema.parse({ ...packageData, id: "store:education", category: "Education & Research" });
    const custom = availablePluginSchema.parse({ ...packageData, id: "store:custom", category: "Custom category" });
    const uncategorized = availablePluginSchema.parse({ ...packageData, id: "store:other" });
    const groups = groupPlugins([productivity, education, { ...productivity, id: "store:second", category: " productivity " }, custom, uncategorized]);
    expect(groups.map(group => group.title)).toEqual(["Produtividade", "Educação e pesquisa", "Custom category", "Mais plugins"]);
    expect(groups[0].plugins.map(plugin => plugin.id)).toEqual(["store:plugin", "store:second"]);
    expect(groups.flatMap(group => group.plugins)).toHaveLength(5);
    for (const iconDataUrl of ["https://example.com/icon.svg", "file:///secret.png", "data:text/html;base64,aWNvbg==", "data:image/svg+xml,<svg/>"]) expect(availablePluginSchema.safeParse({ ...packageData, iconDataUrl }).success).toBe(false);
  });
  it("keeps packages distinguishable when their icons are missing", () => {
    expect(pluginMonogram("Google Drive")).toBe("GD");
    expect(pluginMonogram("Linear")).toBe("LI");
    expect(pluginMonogram("Équipe Marketing")).toBe("ÉM");
    expect(pluginMonogram("---")).toBe("PL");
  });
  it("translates the declared Anthropic taxonomy and preserves unknown source categories", () => {
    const categories = ["development", "productivity", "database", "monitoring", "security", "deployment", "design", "automation", "learning", "location", "testing", "math", "migration", "documentation"];
    expect(categories.map(pluginCategoryLabel)).toEqual(["Desenvolvimento", "Produtividade", "Bancos de dados", "Monitoramento", "Segurança", "Implantação", "Design", "Automação", "Aprendizado", "Localização", "Testes", "Matemática", "Migrações", "Documentação"]);
    expect(pluginCategoryLabel("Experimental Robotics")).toBe("Experimental Robotics");
    expect(pluginCategoryLabel("  DEVELOPMENT  ")).toBe("Desenvolvimento");
  });
});
