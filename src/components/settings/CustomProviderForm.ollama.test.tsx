import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { CustomProviderForm } from "./CustomProviderForm";
import { ollamaAccountFixture } from "@/test/custom-provider-fixtures";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
beforeEach(() => { invoke.mockReset(); });

function discovered(id = "qwen3:14b") {
  return { model: { ...ollamaAccountFixture().custom!.models[0], id, name: id, supportsImages: id === "gemma4:12b" }, sourceUrl: "http://192.168.2.102:11434/api/show", tokenField: "max_tokens" };
}

it("registers only chosen IDs through native Ollama lookup without keys or catalog imports", async () => {
  const user = userEvent.setup(); const onSaved = vi.fn();
  invoke.mockImplementation(async (command: string, args: { modelId?: string }) => command === "lookup_ollama_model" ? discovered(args.modelId) : ollamaAccountFixture());
  render(<CustomProviderForm providerKind="ollama" onSaved={onSaved} onCancel={vi.fn()} />);
  expect(screen.getByLabelText("URL base")).toHaveValue("http://localhost:11434");
  expect(screen.getByLabelText("Nome de exibição")).toHaveValue("Ollama");
  expect(screen.queryByLabelText("Chave de API")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Endpoint")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Compatibilidade avançada|Consultar modelos|Abrir catálogo/ })).not.toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("URL base"), { target: { value: "https://openrouter.ai/api/v1" } });
  expect(screen.queryByRole("button", { name: "Abrir catálogo OpenRouter" })).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("URL base"), { target: { value: "http://192.168.2.102:11434/v1/" } });
  fireEvent.blur(screen.getByLabelText("URL base"));
  for (const [index, id] of [[1, "qwen3:14b"], [2, "gemma4:12b"]] as const) {
    if (index === 2) await user.click(screen.getByRole("button", { name: "Adicionar modelo" }));
    fireEvent.change(screen.getByLabelText(`ID do modelo ${index}`), { target: { value: id } });
    fireEvent.blur(screen.getByLabelText(`ID do modelo ${index}`));
    await waitFor(() => expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeEnabled());
    expect(invoke).toHaveBeenCalledWith("lookup_ollama_model", { baseUrl: "http://192.168.2.102:11434", modelId: id });
  }
  expect(screen.getByRole("button", { name: "Ollama" })).toBeVisible();
  expect(screen.getByText("Imagens de entrada")).toBeVisible();
  expect(screen.queryByText("Geração de imagens")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  await waitFor(() => expect(onSaved).toHaveBeenCalledWith(ollamaAccountFixture()));
  expect(invoke).toHaveBeenLastCalledWith("save_ollama_provider", { alias: "ollama-pessoal", displayName: "Ollama", editing: false, config: expect.objectContaining({ baseUrl: "http://192.168.2.102:11434", protocol: "ollama", authMode: "bearer", tokenField: "max_tokens", replayUnsignedThinking: false, models: [expect.objectContaining({ id: "qwen3:14b", reasoning: "toggle", reasoningLevels: ["off", "on"], defaultReasoningLevel: "on" }), expect.objectContaining({ id: "gemma4:12b", supportsImages: true, supportsImageGeneration: false })] }) });
  expect(invoke).not.toHaveBeenCalledWith("discover_custom_models", expect.anything());
});

it("adds an arbitrary installed ID and saves removal of a registered model", async () => {
  const user = userEvent.setup(); const account = ollamaAccountFixture();
  invoke.mockImplementation(async (command: string) => command === "lookup_ollama_model" ? discovered("my-team/new-model:custom-tag") : account);
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Adicionar modelo" }));
  fireEvent.change(screen.getByLabelText("ID do modelo 2"), { target: { value: "my-team/new-model:custom-tag" } });
  fireEvent.blur(screen.getByLabelText("ID do modelo 2"));
  await waitFor(() => expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Remover modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  expect(invoke).toHaveBeenLastCalledWith("save_ollama_provider", expect.objectContaining({ config: expect.objectContaining({ models: [expect.objectContaining({ id: "my-team/new-model:custom-tag" })] }) }));
});

it("saves required thinking returned by Ollama without offering an unsupported off level", async () => {
  const user = userEvent.setup(); const account = ollamaAccountFixture();
  account.custom!.models[0].defaultReasoningLevel = "off";
  const result = discovered();
  result.model = { ...result.model, reasoningLevels: ["on"], defaultReasoningLevel: "on" };
  invoke.mockImplementation(async (command: string) => command === "lookup_ollama_model" ? result : account);
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Preencher modelo 1 pelo ID" }));
  expect(await screen.findByText("Raciocínio · on")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Configurar manualmente" }));
  await user.click(screen.getByRole("combobox", { name: "Raciocínio padrão · modelo 1" }));
  expect(screen.queryByRole("option", { name: "off" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("option", { name: "on" }));
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  expect(invoke).toHaveBeenLastCalledWith("save_ollama_provider", expect.objectContaining({ config: expect.objectContaining({ models: [expect.objectContaining({ reasoning: "toggle", reasoningLevels: ["on"], defaultReasoningLevel: "on" })] }) }));
});

it("edits context allocation and fixed on/off reasoning without enabling image generation", async () => {
  const user = userEvent.setup(); const account = ollamaAccountFixture(); invoke.mockResolvedValue(account);
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  fireEvent.blur(screen.getByLabelText("ID do modelo 1"));
  expect(invoke).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Configurar manualmente" }));
  expect(screen.getByText(/RAM.*VRAM/)).toHaveTextContent("num_ctx");
  expect(screen.queryByRole("switch", { name: /Geração de imagens/ })).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Níveis aceitos · modelo 1")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Contexto (tokens) · modelo 1"), { target: { value: "32768" } });
  await user.click(screen.getByRole("combobox", { name: "Raciocínio padrão · modelo 1" }));
  await user.click(await screen.findByRole("option", { name: "off" }));
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  expect(invoke).toHaveBeenCalledWith("save_ollama_provider", { alias: account.alias, displayName: "Ollama", editing: true, config: expect.objectContaining({ models: [expect.objectContaining({ contextWindow: 32768, defaultReasoningLevel: "off" })] }) });
});

it.each(["id", "url", "remove"])("ignores a late native lookup after changing %s", async change => {
  const user = userEvent.setup(); const account = ollamaAccountFixture();
  account.custom!.models.push({ ...account.custom!.models[0], id: "gemma4:12b", name: "Gemma" });
  let resolve!: (value: ReturnType<typeof discovered>) => void;
  invoke.mockReturnValue(new Promise<ReturnType<typeof discovered>>(done => { resolve = done; }));
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 2" }));
  await user.click(screen.getByRole("button", { name: "Preencher modelo 2 pelo ID" }));
  expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeDisabled();
  if (change === "id") fireEvent.change(screen.getByLabelText("ID do modelo 2"), { target: { value: "other:latest" } });
  else if (change === "url") fireEvent.change(screen.getByLabelText("URL base"), { target: { value: "http://192.168.2.103:11434" } });
  else await user.click(screen.getByRole("button", { name: "Remover modelo 1" }));
  await act(async () => resolve({ ...discovered("gemma4:12b"), model: { ...discovered("gemma4:12b").model, contextWindow: 65536 } }));
  expect(screen.queryByText("65.536 contexto")).not.toBeInTheDocument();
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeEnabled();
});

it.each(["wrong-id", "wrong-origin"])("rejects an untrusted native metadata result: %s", async mismatch => {
  const user = userEvent.setup();
  const result = discovered();
  if (mismatch === "wrong-id") result.model.id = "another:latest";
  else result.sourceUrl = "http://192.168.2.103:11434/api/show";
  invoke.mockResolvedValue(result);
  render(<CustomProviderForm account={ollamaAccountFixture()} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Preencher modelo 1 pelo ID" }));
  expect(await screen.findByRole("alert")).toBeVisible();
  expect(screen.getByLabelText("Nome do modelo 1")).toHaveValue("qwen3:14b");
});
