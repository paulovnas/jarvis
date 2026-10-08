import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginCatalog, PluginPreview, InstalledPlugin } from "@/core/plugins";
import type { ProviderAccount } from "@/core/provider-accounts";
import { PluginsSettings } from "./PluginsSettings";
import { Tabs as SettingsTabs } from "@base-ui/react/tabs";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const mocked = vi.mocked(invoke);
const installed: InstalledPlugin = { id: "store:example", name: "example", marketplaceId: "store", displayName: "Example", description: "Skills de produção", version: "1.0.0", hash: "hash", rootPath: "/managed/example", dataPath: "/data/example", enabled: true, integrityValid: true, components: [{ id: "skill", name: "Produção", kind: "skills", enabled: true, trusted: true, supported: true, detail: "Direção de arte." }, { id: "hook", name: "Preparação", kind: "hooks", enabled: false, trusted: false, supported: true, detail: "Executa antes da conversa." }, { id: "apps", name: "Serviço hospedado", kind: "apps", enabled: false, trusted: false, supported: false, detail: "Exige acesso ao serviço." }], projectOverrides: {}, warnings: ["Evite duplicar o core nativo."] };
const initial: PluginCatalog = { revision: 3, installed: [], available: [{ id: installed.id, name: installed.name, marketplaceId: "store", displayName: installed.displayName, description: installed.description, version: "1.0.0", source: { kind: "local", path: "/source/example" }, installable: true, authentication: "ON_INSTALL", requirements: ["Node.js 22"] }], marketplaces: [{ id: "store", name: "Comunidade", source: "/source/store", refName: null, sparsePaths: [], refreshed: true, builtIn: false }, { id: "native", name: "Nativo", source: "bundled", refName: null, sparsePaths: [], refreshed: true, builtIn: true }], issues: [], appsAccountId: null };
const preview: PluginPreview = { title: "Revisar Example", description: "Instalar o pacote verificado.", source: "/source/example", hash: "sha256:package", components: installed.components, commands: ['node "${PLUGIN_ROOT}/hooks/start.mjs"'], requirements: ["Node.js 22"], warnings: ["Context-mode já está disponível no core; escolha qual componente usar."], affectedIds: [installed.id] };
let catalog: PluginCatalog; let changed: EventCallback<unknown> | undefined;
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
async function openAdd(label: string) { const user = userEvent.setup(); const add = await screen.findByRole("button", { name: "Adicionar" }); await waitFor(() => expect(add).toBeEnabled()); await user.click(add); await user.click(await screen.findByRole("menuitem", { name: label })); return user; }

describe("PluginsSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks(); catalog = structuredClone(initial); changed = undefined;
    vi.mocked(listen).mockImplementation(async (name, callback) => { if (name === "plugins:changed") changed = callback; return () => { changed = undefined; }; });
    mocked.mockReset().mockImplementation(async command => { if (command === "list_plugins") return catalog; if (command === "preview_plugin_change") return { receiptId: "receipt", revision: catalog.revision, preview }; if (command === "apply_plugin_change") { catalog = { ...catalog, revision: catalog.revision + 1, installed: [installed] }; return catalog; } if (command === "cancel_plugin_change") return; throw new Error(`Unexpected ${command}`); });
  });
  it("shows real store packages, searches and protects native marketplaces", async () => {
    render(<PluginsSettings />);
    expect(await screen.findByRole("button", { name: "Instalar Example" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Remover marketplace Comunidade" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Remover marketplace Nativo" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "Buscar plugins" }), { target: { value: "inexistente" } });
    expect(screen.getByText("Nenhum plugin encontrado")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Instalar Example" })).not.toBeInTheDocument();
    expect(mocked).not.toHaveBeenCalledWith("preview_plugin_change", expect.anything());
  });
  it("keeps the catalog horizontal inside vertical settings and exposes full add choices", async () => {
    const user = userEvent.setup();
    render(<SettingsTabs.Root orientation="vertical" className="group/tabs"><PluginsSettings /></SettingsTabs.Root>);
    const tabs = await screen.findByRole("tablist", { name: "Catálogo de plugins" });
    expect(tabs).toHaveAttribute("aria-orientation", "horizontal");
    expect(within(tabs).getByRole("tab", { name: "Loja" })).toHaveAttribute("aria-selected", "true");
    await user.click(within(tabs).getByRole("tab", { name: /Instalados/ }));
    expect(screen.getByText("Nenhum plugin instalado")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Adicionar" }));
    expect((await screen.findAllByRole("menuitem")).map(item => item.textContent)).toEqual(["Marketplace", "Pasta de plugin", "Arquivo de plugin", "Criar plugin"]);
  });
  it("groups the store by declared categories, preserves search and presents distinct icons in details", async () => {
    const example = { ...initial.available[0], category: "Productivity", shortDescription: "Resumo do pacote", iconDataUrl: "data:image/png;base64,aWNvbg==" };
    catalog = { ...catalog, available: [example, { ...example, id: "store:security", name: "security", displayName: "Security Kit", category: "Security", iconDataUrl: null }, { ...example, id: "store:other", name: "other", displayName: "Outro Plugin", category: null, iconDataUrl: null }] };
    const user = userEvent.setup(); render(<PluginsSettings />);
    const productivity = await screen.findByRole("region", { name: "Produtividade" });
    expect(within(productivity).getByRole("button", { name: "Instalar Example" })).toBeVisible();
    expect(within(productivity).getByText("Resumo do pacote")).toBeVisible();
    expect(within(productivity).getByRole("img", { name: "Ícone de Example" })).toBeVisible();
    expect(within(screen.getByRole("region", { name: "Segurança" })).getByText("SK")).toBeVisible();
    expect(within(screen.getByRole("region", { name: "Mais plugins" })).getByText("OP")).toBeVisible();
    expect(screen.queryByText("Populares")).not.toBeInTheDocument();
    expect(screen.queryByText("Novos imperdíveis")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Detalhes de Example" }));
    const detail = screen.getByRole("dialog", { name: "Example" });
    expect(within(detail).getByRole("img", { name: "Ícone de Example" })).toBeVisible();
    expect(within(detail).getByText("Produtividade")).toBeVisible();
    await user.click(within(detail).getByRole("button", { name: "Fechar" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Buscar plugins" }), { target: { value: "Segurança" } });
    expect(screen.getByRole("button", { name: "Instalar Security Kit" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Instalar Example" })).not.toBeInTheDocument();
  });
  it("preserves installed package identity and category when its marketplace is unavailable", async () => {
    catalog = { ...catalog, available: [], installed: [{ ...installed, category: "Developer Tools", shortDescription: "Direção profissional", iconDataUrl: "data:image/png;base64,aWNvbg==" }] };
    const user = userEvent.setup(); render(<PluginsSettings />);
    await user.click(await screen.findByRole("tab", { name: /Instalados/ }));
    const category = screen.getByRole("region", { name: "Ferramentas de desenvolvimento" });
    expect(within(category).getByText("Direção profissional")).toBeVisible();
    expect(within(category).getByRole("img", { name: "Ícone de Example" })).toBeVisible();
    expect(within(category).getByRole("switch", { name: "Ativar plugin Example" })).toHaveAttribute("aria-checked", "true");
    await user.click(within(category).getByRole("button", { name: "Detalhes de Example" }));
    expect(within(screen.getByRole("dialog", { name: "Example" })).getByRole("img", { name: "Ícone de Example" })).toBeVisible();
  });
  it("previews six plugins per category and opens only the chosen category with a way back", async () => {
    catalog.available = ["Productivity", "Security", null].flatMap((category, group) => Array.from({ length: group === 1 ? 6 : 8 }, (_, index) => ({ ...initial.available[0], id: `store:${group}-${index}`, name: `package-${group}-${index}`, displayName: `Plugin ${group}-${index}`, category })));
    const user = userEvent.setup(); render(<PluginsSettings />);
    const productivity = await screen.findByRole("region", { name: "Produtividade" });
    for (const title of ["Produtividade", "Segurança", "Mais plugins"]) {
      expect(within(screen.getByRole("region", { name: title })).getAllByRole("button", { name: /^Instalar / })).toHaveLength(6);
    }
    expect(screen.queryByRole("button", { name: "Ver mais em Segurança" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Instalar Plugin 0-6" })).not.toBeInTheDocument();
    await user.click(within(productivity).getByRole("button", { name: "Ver mais em Produtividade" }));
    expect(within(screen.getByRole("region", { name: "Produtividade" })).getAllByRole("button", { name: /^Instalar / })).toHaveLength(8);
    expect(screen.queryByRole("region", { name: "Segurança" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Mais plugins" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Marketplaces" })).not.toBeInTheDocument();
    expect(screen.queryByText("Conta para Apps")).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Plugins" })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Voltar à loja" }));
    expect(within(screen.getByRole("region", { name: "Produtividade" })).getAllByRole("button", { name: /^Instalar / })).toHaveLength(6);
    await user.click(screen.getByRole("button", { name: "Ver mais em Mais plugins" }));
    expect(within(screen.getByRole("region", { name: "Mais plugins" })).getAllByRole("button", { name: /^Instalar / })).toHaveLength(8);
    expect(screen.queryByRole("region", { name: "Produtividade" })).not.toBeInTheDocument();
    expect(mocked.mock.calls.map(([command]) => command)).toEqual(["list_plugins"]);
  });
  it("searches all categories without the preview limit, then restores the store when cleared", async () => {
    catalog.available = ["Productivity", "Security"].flatMap((category, group) => Array.from({ length: 8 }, (_, index) => ({ ...initial.available[0], id: `store:${group}-${index}`, name: `package-${group}-${index}`, displayName: `Plugin ${group}-${index}`, category })));
    const user = userEvent.setup(); render(<PluginsSettings />);
    await user.click(await screen.findByRole("button", { name: "Ver mais em Produtividade" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Buscar plugins" }), { target: { value: "Plugin" } });
    expect(screen.getAllByRole("button", { name: /^Instalar / })).toHaveLength(16);
    expect(screen.getByRole("button", { name: "Instalar Plugin 1-7" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /^Ver mais em / })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Voltar à loja" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "Buscar plugins" }), { target: { value: "inexistente" } });
    expect(screen.getByText("Nenhum plugin encontrado")).toBeVisible();
    fireEvent.change(screen.getByRole("textbox", { name: "Buscar plugins" }), { target: { value: "" } });
    expect(screen.getAllByRole("button", { name: /^Instalar / })).toHaveLength(12);
    expect(screen.getByRole("button", { name: "Ver mais em Segurança" })).toBeVisible();
  });
  it("keeps installed plugins complete and resets the category when changing tabs", async () => {
    catalog.available = Array.from({ length: 8 }, (_, index) => ({ ...initial.available[0], id: `store:${index}`, name: `package-${index}`, displayName: `Plugin ${index}`, category: "Productivity" }));
    catalog.installed = catalog.available.map(plugin => ({ ...installed, id: plugin.id, name: plugin.name, displayName: plugin.displayName, category: plugin.category }));
    const user = userEvent.setup(); render(<PluginsSettings />);
    await user.click(await screen.findByRole("button", { name: "Ver mais em Produtividade" }));
    await user.click(screen.getByRole("tab", { name: /Instalados/ }));
    expect(screen.getAllByRole("switch", { name: /^Ativar plugin / })).toHaveLength(8);
    expect(screen.queryByRole("button", { name: /^Ver mais em / })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Loja" }));
    expect(screen.getAllByRole("switch", { name: /^Ativar plugin / })).toHaveLength(6);
    expect(screen.queryByRole("button", { name: "Voltar à loja" })).not.toBeInTheDocument();
  });
  it("returns to the available categories if a catalog refresh removes the open category", async () => {
    catalog.available = Array.from({ length: 7 }, (_, index) => ({ ...initial.available[0], id: `store:${index}`, name: `package-${index}`, displayName: `Plugin ${index}`, category: "Productivity" }));
    const user = userEvent.setup(); render(<PluginsSettings />);
    await user.click(await screen.findByRole("button", { name: "Ver mais em Produtividade" }));
    catalog = { ...catalog, revision: catalog.revision + 1, available: [{ ...initial.available[0], category: "Security" }] };
    await act(async () => changed?.({ event: "plugins:changed", id: 1, payload: {} }));
    expect(await screen.findByRole("region", { name: "Segurança" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Voltar à loja" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Marketplaces" })).toBeVisible();
    catalog = { ...catalog, revision: catalog.revision + 1, available: [...catalog.available, ...Array.from({ length: 7 }, (_, index) => ({ ...initial.available[0], id: `store:${index}`, name: `package-${index}`, displayName: `Plugin ${index}`, category: "Productivity" }))] };
    await act(async () => changed?.({ event: "plugins:changed", id: 2, payload: {} }));
    expect(within(await screen.findByRole("region", { name: "Produtividade" })).getAllByRole("button", { name: /^Instalar / })).toHaveLength(6);
    expect(screen.getByRole("region", { name: "Segurança" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Voltar à loja" })).not.toBeInTheDocument();
  });
  it("shows exact command, requirements and collisions before explicit install", async () => {
    const user = userEvent.setup(); const onBusyChange = vi.fn(); render(<PluginsSettings onBusyChange={onBusyChange} />);
    await user.click(await screen.findByRole("button", { name: "Instalar Example" }));
    const review = await screen.findByRole("dialog", { name: preview.title });
    expect(within(review).getByText(preview.commands[0])).toBeVisible();
    expect(within(review).getByText("Node.js 22")).toBeVisible();
    expect(within(review).getByText(preview.warnings[0])).toBeVisible();
    expect(mocked).toHaveBeenCalledWith("preview_plugin_change", { operation: { action: "install", pluginId: installed.id }, expectedRevision: 3 });
    expect(mocked).not.toHaveBeenCalledWith("apply_plugin_change", expect.anything());
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: preview.title })).toBeVisible();
    await user.click(within(review).getByRole("button", { name: "Confirmar alteração" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(mocked).toHaveBeenCalledWith("apply_plugin_change", { receiptId: "receipt", expectedRevision: 3 });
    expect(screen.getByRole("tab", { name: /Instalados/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("switch", { name: "Ativar plugin Example" })).toHaveAttribute("aria-checked", "true");
    expect(onBusyChange).toHaveBeenCalledWith(true);
    expect(onBusyChange).toHaveBeenLastCalledWith(false);
  });
  it("cancels the native prepared receipt without installing", async () => {
    const user = userEvent.setup(); render(<PluginsSettings />);
    await user.click(await screen.findByRole("button", { name: "Instalar Example" }));
    const review = await screen.findByRole("dialog", { name: preview.title });
    await user.click(within(review).getByRole("button", { name: "Cancelar" }));
    expect(mocked).toHaveBeenCalledWith("cancel_plugin_change", { receiptId: "receipt" });
    expect(mocked).not.toHaveBeenCalledWith("apply_plugin_change", expect.anything());
  });
  it("retains review on stale apply and requires a fresh explicit review", async () => {
    const user = userEvent.setup(); render(<PluginsSettings />);
    await user.click(await screen.findByRole("button", { name: "Instalar Example" }));
    const review = await screen.findByRole("dialog", { name: preview.title });
    catalog = { ...catalog, revision: 7 };
    await act(async () => changed?.({ event: "plugins:changed", id: 1, payload: {} }));
    mocked.mockImplementationOnce(async () => { throw { code: "stale_revision", message: "A configuração mudou. Revise novamente." }; });
    await user.click(within(review).getByRole("button", { name: "Confirmar alteração" }));
    expect(await within(review).findByText("A configuração mudou. Revise novamente.")).toBeVisible();
    expect(mocked).toHaveBeenCalledWith("apply_plugin_change", { receiptId: "receipt", expectedRevision: 3 });
    await user.click(screen.getByRole("button", { name: "Revisar novamente" }));
    await screen.findByRole("button", { name: "Confirmar alteração" });
    expect(mocked).toHaveBeenLastCalledWith("preview_plugin_change", { operation: { action: "install", pluginId: installed.id }, expectedRevision: 7 });
  });
  it("preserves source form input and its captured revision across catalog events", async () => {
    render(<PluginsSettings />); const user = await openAdd("Marketplace");
    fireEvent.change(screen.getByRole("textbox", { name: "Origem do marketplace" }), { target: { value: "org/market" } });
    fireEvent.change(screen.getByRole("textbox", { name: /Branch, tag/ }), { target: { value: "v2" } });
    fireEvent.change(screen.getByRole("textbox", { name: /Pastas para checkout/ }), { target: { value: "plugins\nshared" } });
    catalog = { ...catalog, revision: 6 }; await act(async () => changed?.({ event: "plugins:changed", id: 1, payload: {} }));
    mocked.mockImplementationOnce(async () => { throw { code: "stale_revision", message: "Reabra o formulário para revisar a versão atual." }; });
    await user.click(screen.getByRole("button", { name: "Revisar marketplace" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Reabra o formulário");
    expect(screen.getByRole("textbox", { name: "Origem do marketplace" })).toHaveValue("org/market");
    expect(mocked).toHaveBeenCalledWith("preview_plugin_change", { operation: { action: "addMarketplace", source: "https://github.com/org/market.git", refName: "v2", sparsePaths: ["plugins", "shared"] }, expectedRevision: 3 });
  });
  it("imports a selected archive only through the review operation", async () => {
    vi.mocked(open).mockResolvedValueOnce("/downloads/plugin.zip");
    render(<PluginsSettings />); await openAdd("Arquivo de plugin");
    await screen.findByRole("dialog", { name: preview.title });
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: false, multiple: false, filters: [expect.objectContaining({ extensions: ["zip", "tar", "gz", "tgz"] })] }));
    expect(mocked).toHaveBeenCalledWith("preview_plugin_change", { operation: { action: "import", path: "/downloads/plugin.zip" }, expectedRevision: 3 });
    expect(mocked).not.toHaveBeenCalledWith("apply_plugin_change", expect.anything());
  });
  it("creates a managed package and preserves SKILL.md literally", async () => {
    render(<PluginsSettings />); const user = await openAdd("Criar plugin");
    fireEvent.change(screen.getByRole("textbox", { name: "Nome do plugin" }), { target: { value: "marketing" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Descrição" }), { target: { value: "Produção de vídeos" } });
    fireEvent.change(screen.getByRole("textbox", { name: /Nome da skill/ }), { target: { value: "video" } });
    const content = "---\nname: video\ndescription: Vídeos\n---\nPreserve $literal.\n";
    fireEvent.change(screen.getByRole("textbox", { name: "Conteúdo de SKILL.md" }), { target: { value: content } });
    await user.click(screen.getByRole("button", { name: "Revisar plugin" }));
    await screen.findByRole("dialog", { name: preview.title });
    expect(mocked).toHaveBeenCalledWith("preview_plugin_change", { operation: { action: "create", draft: { name: "marketing", description: "Produção de vídeos", skills: [{ name: "video", content }], mcpServers: {}, hooks: null, apps: {}, files: [] } }, expectedRevision: 3 });
  });
  it("keeps hooks separately untrusted and requires exact review before trusting", async () => {
    catalog = { ...catalog, installed: [installed] }; const user = userEvent.setup(); render(<PluginsSettings />);
    await user.click(await screen.findByRole("button", { name: "Detalhes de Example" }));
    const details = screen.getByRole("dialog", { name: "Example" });
    expect(within(details).getByText(/Comandos ainda não autorizados/)).toBeVisible();
    expect(within(details).getByRole("switch", { name: "Ativar componente Serviço hospedado" })).toHaveAttribute("aria-disabled", "true");
    await user.click(within(details).getByRole("button", { name: "Revisar e autorizar hooks" }));
    const review = await screen.findByRole("dialog", { name: preview.title });
    expect(within(review).getByText(preview.commands[0])).toBeVisible();
    expect(mocked).toHaveBeenCalledWith("preview_plugin_change", { operation: { action: "trustHooks", pluginId: installed.id, trusted: true }, expectedRevision: 3 });
    expect(mocked).not.toHaveBeenCalledWith("apply_plugin_change", expect.anything());
  });
  it("reviews component changes and uninstall instead of saving immediately", async () => {
    catalog = { ...catalog, installed: [installed] }; const user = userEvent.setup(); render(<PluginsSettings />);
    await user.click(await screen.findByRole("button", { name: "Detalhes de Example" }));
    await user.click(screen.getByRole("switch", { name: "Ativar componente Produção" }));
    await screen.findByRole("dialog", { name: preview.title });
    expect(mocked).toHaveBeenCalledWith("preview_plugin_change", { operation: { action: "configureComponent", pluginId: installed.id, componentId: "skill", enabled: false }, expectedRevision: 3 });
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    await user.click(screen.getByRole("button", { name: "Detalhes de Example" }));
    await user.click(screen.getByRole("button", { name: "Desinstalar" }));
    await screen.findByRole("dialog", { name: preview.title });
    expect(mocked).toHaveBeenCalledWith("preview_plugin_change", { operation: { action: "uninstall", pluginId: installed.id }, expectedRevision: 3 });
    expect(mocked).not.toHaveBeenCalledWith("apply_plugin_change", expect.anything());
  });
  it("blocks duplicate apply and reports pending state until the native operation finishes", async () => {
    const pending = deferred<PluginCatalog>(); const user = userEvent.setup(); const busy = vi.fn(); render(<PluginsSettings onBusyChange={busy} />);
    await user.click(await screen.findByRole("button", { name: "Instalar Example" }));
    mocked.mockImplementationOnce(() => pending.promise);
    const confirm = await screen.findByRole("button", { name: "Confirmar alteração" });
    await user.dblClick(confirm);
    expect(mocked.mock.calls.filter(([command]) => command === "apply_plugin_change")).toHaveLength(1);
    expect(confirm).toBeDisabled(); expect(busy).toHaveBeenLastCalledWith(true);
    await act(async () => pending.resolve({ ...catalog, revision: 4, installed: [installed] }));
    await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  });
  it("shows altered packages as suspended and does not blindly reenable them", async () => {
    catalog = { ...catalog, installed: [{ ...installed, integrityValid: false }] }; render(<PluginsSettings />);
    const enabled = await screen.findByRole("switch", { name: "Ativar plugin Example" });
    expect(enabled).toHaveAttribute("aria-checked", "false"); expect(enabled).toHaveAttribute("aria-disabled", "true");
  });
  it("shows only enabled ChatGPT accounts and requires an explicit account change review", async () => {
    const make = (alias: string, providerKind: string, enabled = true): ProviderAccount => ({ alias, providerKind, enabled, createdAt: 0, email: null, accountType: "personal", models: [], modelsAvailable: true });
    const user = userEvent.setup(); render(<PluginsSettings accounts={[make("chatgpt", "openai-codex"), make("desativada", "openai-codex", false), make("outro", "opencode-go")]} />);
    await user.click(await screen.findByRole("combobox", { name: "Conta ChatGPT" }));
    expect(screen.queryByRole("option", { name: "desativada" })).not.toBeInTheDocument(); expect(screen.queryByRole("option", { name: "outro" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("option", { name: "chatgpt" }));
    await screen.findByRole("dialog", { name: preview.title });
    expect(mocked).toHaveBeenCalledWith("preview_plugin_change", { operation: { action: "setAppsAccount", accountId: "chatgpt" }, expectedRevision: 3 });
  });
  it("exposes app authorization and holds the settings detail open during gateway tests", async () => {
    const connectUrl = "https://chatgpt.com/apps/drive/connector-one";
    catalog = { ...catalog, appsAccountId: "chatgpt", installed: [{ ...installed, components: [{ id: "apps:drive", name: "Drive", kind: "apps", enabled: true, supported: true, trusted: true, detail: "Drive connector", appConnectUrl: connectUrl, mcpServerId: "plugin-app:gateway" }] }] };
    const pending = deferred<unknown>(); mocked.mockImplementation(command => command === "list_plugins" ? Promise.resolve(catalog) : command === "test_mcp_server" ? pending.promise : Promise.reject(new Error(`Unexpected ${command}`)));
    const busy = vi.fn(); const user = userEvent.setup(); render(<PluginsSettings onBusyChange={busy} accounts={[{ alias: "chatgpt", providerKind: "openai-codex", enabled: true, createdAt: 0, email: null, accountType: "personal", models: [], modelsAvailable: true }]} />);
    await user.click(await screen.findByRole("button", { name: "Detalhes de Example" }));
    expect(screen.getByText("Conexão ainda não verificada")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Autorizar no ChatGPT" })); expect(openUrl).toHaveBeenCalledExactlyOnceWith(connectUrl); expect(screen.queryByText("Gateway verificado")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Testar conexão" })); expect(mocked).toHaveBeenCalledWith("test_mcp_server", { id: "plugin-app:gateway" }); expect(busy).toHaveBeenLastCalledWith(true); expect(screen.getByRole("button", { name: "Fechar" })).toBeDisabled();
    await user.keyboard("{Escape}"); expect(screen.getByRole("dialog", { name: "Example" })).toBeVisible();
    await act(async () => pending.resolve({ toolCount: 0, tools: [], error: "Autorize Drive na conta ChatGPT." }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Autorize Drive"); await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false)); expect(screen.getByRole("button", { name: "Fechar" })).toBeEnabled();
  });
  it("refreshes OAuth, private requirements and gateway tests when an installed package changes", async () => {
    const mcp = { id: "mcp:remote", name: "Remote", kind: "mcp" as const, enabled: true, supported: true, trusted: true, detail: "Remote server", mcpServerId: "plugin-mcp:remote", mcpOAuth: true };
    const app = { id: "apps:drive", name: "Drive", kind: "apps" as const, enabled: true, supported: true, trusted: true, detail: "Drive connector", appConnectUrl: "https://chatgpt.com/apps/drive/connector-one", mcpServerId: "plugin-app:gateway" };
    catalog = { ...catalog, appsAccountId: "chatgpt", installed: [{ ...installed, hash: "original-hash", components: [mcp, app] }] };
    mocked.mockImplementation(async command => {
      const original = catalog.installed[0].hash === "original-hash";
      if (command === "list_plugins") return catalog;
      if (command === "plugin_mcp_requirements") return { fields: [original ? "OLD_KEY" : "NEW_KEY"], configured: original };
      if (command === "mcp_oauth_status") return { authenticated: original, state: original ? "connected" : "disconnected", error: null };
      if (command === "test_mcp_server") return { toolCount: 2, tools: ["search", "read"], error: null };
      throw new Error(`Unexpected ${command}`);
    });
    const user = userEvent.setup(); render(<PluginsSettings accounts={[{ alias: "chatgpt", providerKind: "openai-codex", enabled: true, createdAt: 0, email: null, accountType: "personal", models: [], modelsAvailable: true }]} />);
    await user.click(await screen.findByRole("button", { name: "Detalhes de Example" })); expect(await screen.findByText("Conectado")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Testar conexão" })); expect(await screen.findByText("Gateway verificado")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Alterar credenciais" })); fireEvent.change(screen.getByLabelText("OLD_KEY"), { target: { value: "unsaved-local-value" } });
    catalog = { ...catalog, revision: 4, installed: [{ ...catalog.installed[0], hash: "updated-hash" }] };
    await act(async () => changed?.({ event: "plugins:changed", id: 1, payload: {} }));
    expect(await screen.findByText("Conta não conectada")).toBeVisible(); expect(screen.queryByText("Gateway verificado")).not.toBeInTheDocument(); expect(screen.getByText("Conexão ainda não verificada")).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "Configurar Remote" })).not.toBeInTheDocument(); expect(mocked.mock.calls.filter(([command]) => command === "mcp_oauth_status")).toHaveLength(2); expect(mocked.mock.calls.filter(([command]) => command === "plugin_mcp_requirements")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Configurar credenciais" })); expect(screen.getByLabelText("NEW_KEY")).toHaveValue(""); expect(screen.queryByLabelText("OLD_KEY")).not.toBeInTheDocument(); expect(mocked).not.toHaveBeenCalledWith("configure_plugin_mcp", expect.anything());
  });
  it("cancels an old OAuth flow on package update and ignores its late response", async () => {
    catalog = { ...catalog, installed: [{ ...installed, hash: "original-hash", components: [{ id: "mcp:remote", name: "Remote", kind: "mcp", enabled: true, supported: true, trusted: true, detail: "Remote server", mcpServerId: "plugin-mcp:remote", mcpOAuth: true }] }] };
    const pending = deferred<unknown>(); mocked.mockImplementation(async command => {
      if (command === "list_plugins") return catalog;
      if (command === "plugin_mcp_requirements") return { fields: [], configured: true };
      if (command === "mcp_oauth_status") return { authenticated: false, state: "disconnected", error: null };
      if (command === "start_mcp_oauth") return { flowId: "old-flow", authorizationUrl: "https://server.example/authorize" };
      if (command === "wait_mcp_oauth") return pending.promise;
      if (command === "cancel_mcp_oauth") return;
      throw new Error(`Unexpected ${command}`);
    });
    const busy = vi.fn(); const user = userEvent.setup(); render(<PluginsSettings onBusyChange={busy} />);
    await user.click(await screen.findByRole("button", { name: "Detalhes de Example" })); await user.click(await screen.findByRole("button", { name: "Conectar conta" })); expect(await screen.findByText("Aguardando navegador")).toBeVisible(); expect(busy).toHaveBeenLastCalledWith(true);
    catalog = { ...catalog, revision: 4, installed: [{ ...catalog.installed[0], hash: "updated-hash" }] }; await act(async () => changed?.({ event: "plugins:changed", id: 1, payload: {} }));
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("cancel_mcp_oauth", { flowId: "old-flow" })); expect(await screen.findByText("Conta não conectada")).toBeVisible(); await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
    await act(async () => pending.resolve({ authenticated: true, state: "connected", error: null })); expect(screen.queryByText("Conectado")).not.toBeInTheDocument(); expect(screen.getByText("Conta não conectada")).toBeVisible();
  });
});
