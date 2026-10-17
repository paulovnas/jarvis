import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { CustomModelDiscovery } from "./CustomModelDiscovery";
import { customConfigFixture } from "@/test/custom-provider-fixtures";
import type { CustomConfig } from "@/core/custom-provider";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
beforeEach(() => invoke.mockReset());
const entry = (id: string) => ({ id, name: id, contextWindow: null, maxOutputTokens: null, supportsTools: null, supportsImages: null });
const props = () => ({ config: customConfigFixture(), apiKey: "synthetic-key", disabled: false, onPendingChange: vi.fn(), onImport: vi.fn() });

it.each<CustomConfig["protocol"]>(["openai-completions", "openai-responses", "anthropic-messages"])("importa IDs literais em lote via %s sem inventar limites ou recursos", async protocol => {
  const user = userEvent.setup(); const options = props(); options.config.protocol = protocol;
  invoke.mockResolvedValue({ models: [entry("Opus 5.5"), entry("GPT-5.6"), entry("opus 5.5"), entry("vendor/model")] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  expect(invoke).toHaveBeenCalledWith("discover_custom_models", { target: { baseUrl: options.config.baseUrl, protocol, authMode: "bearer" }, apiKey: "synthetic-key", alias: null });
  expect(await screen.findByText(/4 modelos no catálogo · 1 já cadastrados/)).toBeVisible();
  expect(screen.queryByRole("checkbox", { name: "vendor/model" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados/ }));
  expect(screen.getByRole("alert")).toHaveTextContent("Informe contexto");
  expect(options.onImport).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("Contexto comum (tokens)"), { target: { value: "64000" } });
  fireEvent.change(screen.getByLabelText("Saída máxima comum (tokens)"), { target: { value: "4000" } });
  expect(screen.getByRole("switch", { name: "Ferramentas nos modelos sem informação" })).not.toBeChecked();
  await user.click(screen.getByRole("switch", { name: "Ferramentas nos modelos sem informação" }));
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados/ }));
  expect(options.onImport).toHaveBeenCalledWith(["Opus 5.5", "GPT-5.6", "opus 5.5"].map(id => ({ id, name: id, contextWindow: 64000, maxOutputTokens: 4000, supportsTools: true, supportsImages: false, reasoning: "none", reasoningLevels: [], defaultReasoningLevel: null, thinkingBudget: null })));
});

it("usa metadados informados, permite selecionar e não substitui a chave salva", async () => {
  const user = userEvent.setup(); const options = { ...props(), apiKey: "", alias: "gateway" };
  invoke.mockResolvedValue({ models: [{ ...entry("Opus 5"), contextWindow: 32000, maxOutputTokens: 2000, supportsTools: true, supportsImages: false }, entry("Outro")] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  await screen.findByRole("checkbox", { name: "Outro" });
  await user.click(screen.getByRole("checkbox", { name: "Outro" }));
  expect(screen.queryByLabelText("Contexto comum (tokens)")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados \(1\)/ }));
  expect(invoke).toHaveBeenCalledWith("discover_custom_models", expect.objectContaining({ apiKey: null, alias: "gateway" }));
  expect(options.onImport).toHaveBeenCalledWith([expect.objectContaining({ id: "Opus 5", contextWindow: 32000, maxOutputTokens: 2000, supportsTools: true })]);
});

it.each(["url", "key", "protocol"])("ignora catálogo atrasado após alterar %s e libera o salvamento", async change => {
  let resolve!: (value: unknown) => void;
  invoke.mockReturnValue(new Promise(done => { resolve = done; }));
  const user = userEvent.setup(); const options = props();
  const view = render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  expect(screen.getByRole("status", { name: "Consultando catálogo de modelos" })).toBeVisible();
  view.rerender(<CustomModelDiscovery {...options} apiKey={change === "key" ? "next-key" : options.apiKey} config={{ ...options.config, baseUrl: change === "url" ? "https://other.example/v1" : options.config.baseUrl, protocol: change === "protocol" ? "anthropic-messages" : options.config.protocol }} />);
  await act(async () => resolve({ models: [entry("Opus 5.5")] }));
  expect(screen.queryByText("Opus 5.5")).not.toBeInTheDocument();
  expect(options.onPendingChange).toHaveBeenLastCalledWith(false);
});

it("exibe erro de autenticação sem alterar os modelos existentes e permite nova consulta", async () => {
  const user = userEvent.setup(); const options = props();
  invoke.mockRejectedValueOnce({ message: "O catálogo recusou a autenticação (401/403)." }).mockResolvedValue({ models: [] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("401/403");
  expect(options.onImport).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  expect(await screen.findByText(/0 modelos no catálogo/)).toBeVisible();
});

it("impede importar mais de 100 modelos sem truncar silenciosamente o catálogo", async () => {
  const user = userEvent.setup(); const options = props();
  invoke.mockResolvedValue({ models: Array.from({ length: 100 }, (_, index) => ({ ...entry(`model ${index}`), contextWindow: 32000, maxOutputTokens: 2000 })) });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  await user.click(await screen.findByRole("button", { name: /Importar modelos selecionados/ }));
  expect(screen.getByRole("alert")).toHaveTextContent("até 100");
  expect(options.onImport).not.toHaveBeenCalled();
});

it.each<CustomConfig["protocol"]>(["openai-completions", "openai-responses", "anthropic-messages"])("importa limites independentes de entrada e saída via %s", async protocol => {
  const user = userEvent.setup(); const options = props(); options.config.protocol = protocol;
  invoke.mockResolvedValue({ models: [{ ...entry("minimax-m3"), name: "MiniMax M3", contextWindow: 512000, maxOutputTokens: 512000, supportsTools: true, supportsImages: true }, { ...entry("large-output"), contextWindow: 4096, maxOutputTokens: 8192, supportsTools: false, supportsImages: false }] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  await user.click(await screen.findByRole("button", { name: /Importar modelos selecionados/ }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(options.onImport).toHaveBeenCalledWith([expect.objectContaining({ id: "minimax-m3", name: "MiniMax M3", contextWindow: 512000, maxOutputTokens: 512000, supportsImages: true }), expect.objectContaining({ id: "large-output", contextWindow: 4096, maxOutputTokens: 8192 })]);
});

it("identifica todos os limites inválidos e permite corrigir só esses modelos antes de importar os 22", async () => {
  const user = userEvent.setup(); const options = props();
  const models = Array.from({ length: 22 }, (_, index) => ({ ...entry(`model ${index}`), contextWindow: 512000, maxOutputTokens: 512000, supportsTools: true, supportsImages: false }));
  models[5].contextWindow = 1024; models[17].maxOutputTokens = 10_000_001;
  invoke.mockResolvedValue({ models });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  await user.click(await screen.findByRole("button", { name: /Importar modelos selecionados \(22\)/ }));
  expect(screen.getByRole("alert")).toHaveTextContent("model 5");
  expect(screen.getByRole("alert")).toHaveTextContent("model 17");
  expect(options.onImport).not.toHaveBeenCalled();
  expect(screen.getByLabelText("Contexto (tokens) · model 5")).toHaveValue(1024);
  expect(screen.getByLabelText("Saída máxima (tokens) · model 17")).toHaveValue(10_000_001);
  fireEvent.change(screen.getByLabelText("Contexto (tokens) · model 5"), { target: { value: "8192" } });
  fireEvent.change(screen.getByLabelText("Saída máxima (tokens) · model 17"), { target: { value: "32000" } });
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados \(22\)/ }));
  expect(options.onImport).toHaveBeenCalledTimes(1);
  const imported = options.onImport.mock.calls[0][0] as CustomConfig["models"];
  expect(imported).toHaveLength(22);
  expect(imported[5]).toMatchObject({ contextWindow: 8192, maxOutputTokens: 512000 });
  expect(imported[17]).toMatchObject({ contextWindow: 512000, maxOutputTokens: 32000 });
  expect(imported[0]).toMatchObject({ contextWindow: 512000, maxOutputTokens: 512000 });
  expect(options.config.models).toEqual(customConfigFixture().models);
});

it.each<CustomConfig["protocol"]>(["openai-completions", "openai-responses"])("importa visão e esforços documentados via %s sem inventar ferramentas", async protocol => {
  const user = userEvent.setup(); const options = props(); options.config.protocol = protocol;
  invoke.mockResolvedValue({ models: [{ ...entry("DeepSeek V4"), name: "DeepSeek-V4-Pro", contextWindow: 1_000_000, maxOutputTokens: 384000, supportsImages: false, reasoningLevels: [{ value: "high", display: "High" }, { value: "xhigh", display: "X-High" }], defaultReasoningLevel: "high" }] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  expect(await screen.findByRole("switch", { name: "Ferramentas nos modelos sem informação" })).not.toBeChecked();
  expect(screen.queryByRole("switch", { name: "Imagens nos modelos sem informação" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados/ }));
  expect(options.onImport).toHaveBeenCalledWith([expect.objectContaining({ id: "DeepSeek V4", name: "DeepSeek-V4-Pro", contextWindow: 1_000_000, maxOutputTokens: 384000, supportsImages: false, supportsTools: false, reasoning: "effort", reasoningLevels: ["high", "xhigh"], defaultReasoningLevel: "high" })]);
});

it("permite completar limites diferentes por modelo sem aplicar valores comuns", async () => {
  const user = userEvent.setup(); const options = props();
  invoke.mockResolvedValue({ models: [entry("first"), entry("second")] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  await screen.findByRole("button", { name: /Importar modelos selecionados/ });
  for (const [id, contextWindow, maxOutputTokens] of [["first", "32000", "4000"], ["second", "64000", "8000"]]) {
    fireEvent.change(screen.getByLabelText(`Contexto (tokens) · ${id}`), { target: { value: contextWindow } });
    fireEvent.change(screen.getByLabelText(`Saída máxima (tokens) · ${id}`), { target: { value: maxOutputTokens } });
  }
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados/ }));
  expect(options.onImport).toHaveBeenCalledWith([expect.objectContaining({ id: "first", contextWindow: 32000, maxOutputTokens: 4000 }), expect.objectContaining({ id: "second", contextWindow: 64000, maxOutputTokens: 8000 })]);
});

it("mostra os valores numéricos inválidos publicados, incluindo zero, para correção", async () => {
  const user = userEvent.setup(); const options = props();
  invoke.mockResolvedValue({ models: [{ ...entry("bad-limits"), contextWindow: -1, maxOutputTokens: 0 }] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  expect(await screen.findByLabelText("Contexto (tokens) · bad-limits")).toHaveValue(-1);
  expect(screen.getByLabelText("Saída máxima (tokens) · bad-limits")).toHaveValue(0);
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados/ }));
  expect(screen.getByRole("alert")).toHaveTextContent("bad-limits");
  expect(options.onImport).not.toHaveBeenCalled();
});

it.each(["Thinking adaptativo", "Padrão do provedor"])("confirma o formato de Messages sem deduzir o dialeto: %s", async label => {
  const user = userEvent.setup(); const options = props(); options.config.protocol = "anthropic-messages";
  invoke.mockResolvedValue({ models: [{ ...entry("reasoner"), contextWindow: 64000, maxOutputTokens: 4000, reasoningLevels: [{ value: "low", display: "Baixo" }, { value: "high", display: "Alto" }], defaultReasoningLevel: "high" }] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  await user.click(await screen.findByRole("button", { name: /Importar modelos selecionados/ }));
  expect(screen.getByRole("alert")).toHaveTextContent("Confirme o formato");
  expect(screen.getByRole("alert")).toHaveTextContent("reasoner");
  expect(options.onImport).not.toHaveBeenCalled();
  await user.click(screen.getByRole("combobox", { name: "Formato de raciocínio · reasoner" }));
  await user.click(await screen.findByRole("option", { name: label }));
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados/ }));
  expect(options.onImport).toHaveBeenCalledWith([expect.objectContaining(label === "Thinking adaptativo" ? { reasoning: "adaptive", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" } : { reasoning: "none", reasoningLevels: [], defaultReasoningLevel: null })]);
});

it("permite escolher o padrão ausente pelos rótulos publicados sem inventar um esforço", async () => {
  const user = userEvent.setup(); const options = props();
  invoke.mockResolvedValue({ models: [{ ...entry("reasoner"), contextWindow: 64000, maxOutputTokens: 4000, reasoningLevels: [{ value: "low", display: "Baixo publicado" }, { value: "high", display: "Alto publicado" }], defaultReasoningLevel: null }] });
  render(<CustomModelDiscovery {...options} />);
  await user.click(screen.getByRole("button", { name: "Descobrir modelos" }));
  await user.click(await screen.findByRole("button", { name: /Importar modelos selecionados/ }));
  expect(screen.getByRole("alert")).toHaveTextContent("reasoner");
  expect(options.onImport).not.toHaveBeenCalled();
  await user.click(screen.getByRole("combobox", { name: "Raciocínio padrão · reasoner" }));
  await user.click(await screen.findByRole("option", { name: "Alto publicado" }));
  await user.click(screen.getByRole("button", { name: /Importar modelos selecionados/ }));
  expect(options.onImport).toHaveBeenCalledWith([expect.objectContaining({ reasoning: "effort", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" })]);
});
