import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { CustomProviderForm } from "./CustomProviderForm";
import { customAccountFixture } from "@/test/custom-provider-fixtures";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
beforeEach(() => { invoke.mockReset(); });
it("salva alias, endpoint e limites sem iniciar autenticação ou descoberta remota", async () => {
  const user = userEvent.setup(); const onSaved = vi.fn(); const account = customAccountFixture();
  invoke.mockResolvedValue(account);
  render(<CustomProviderForm onSaved={onSaved} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  expect(screen.getByRole("alert")).toHaveTextContent("Alias"); expect(invoke).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Configurar manualmente" }));
  for (const [label, value] of [["Alias completo",account.alias], ["URL base",account.custom!.baseUrl], ["Chave de API","private-key"], ["ID do modelo 1","vendor/model"], ["Nome do modelo 1","Meu modelo"], ["Contexto (tokens) · modelo 1","64000"], ["Saída máxima (tokens) · modelo 1","4000"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  await user.click(screen.getByRole("combobox", { name: "Endpoint" }));
  await user.click(await screen.findByRole("option", { name: "Anthropic · Messages" }));
  await user.click(screen.getByRole("button", { name: "Compatibilidade avançada" }));
  expect(screen.getByRole("combobox", { name: "Autenticação" })).toHaveTextContent("Bearer");
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  await waitFor(() => expect(onSaved).toHaveBeenCalledWith(account));
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(invoke).toHaveBeenCalledWith("save_custom_provider", expect.objectContaining({ alias: account.alias, apiKey: "private-key", editing: false, config: expect.objectContaining({ protocol: "anthropic-messages", models: [expect.objectContaining({ id: "vendor/model", contextWindow: 64000, maxOutputTokens: 4000 })] }) }));
});
it("edita modelos sem expor ou substituir a chave armazenada", async () => {
  const user = userEvent.setup(); const account = customAccountFixture(); invoke.mockResolvedValue(account);
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.getByLabelText("Alias completo")).toBeDisabled();
  expect(screen.getByLabelText("Chave de API")).toHaveValue("");
  await user.click(screen.getByRole("button", { name: "Adicionar modelo" }));
  await user.click(screen.getByRole("button", { name: "Configurar manualmente" }));
  expect(screen.getByLabelText("Contexto (tokens) · modelo 2")).toHaveValue(null);
  await user.click(screen.getByRole("button", { name: "Remover modelo 2" }));
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Configurar manualmente" }));
  fireEvent.change(screen.getByLabelText("Nome do modelo 1"), { target: { value: "Novo nome" } });
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_custom_provider", expect.objectContaining({ editing: true, apiKey: null, config: expect.objectContaining({ models: [expect.objectContaining({ name: "Novo nome" })] }) })));
});
it("oferece somente os níveis cadastrados e impede salvar sem escolher o padrão", async () => {
  const user = userEvent.setup(); const account = customAccountFixture(); invoke.mockResolvedValue(account);
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Configurar manualmente" }));
  await user.click(screen.getByRole("combobox", { name: "Raciocínio · modelo 1" }));
  await user.click(await screen.findByRole("option", { name: "OpenRouter · reasoning" }));
  fireEvent.change(screen.getByLabelText("Níveis aceitos · modelo 1"), { target: { value: "low,high" } });
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  expect(screen.getByRole("alert")).toHaveTextContent("padrão");
  await user.click(screen.getByRole("combobox", { name: "Raciocínio padrão · modelo 1" }));
  expect(screen.queryByRole("option", { name: "medium" })).not.toBeInTheDocument();
  await user.click(await screen.findByRole("option", { name: "high" }));
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_custom_provider", expect.objectContaining({ config: expect.objectContaining({ models: [expect.objectContaining({ reasoning: "openrouter", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" })] }) })));
});

function openRouterAccount() {
  const account = customAccountFixture();
  account.custom!.baseUrl = "https://openrouter.ai/api/v1";
  account.custom!.models[0] = { ...account.custom!.models[0], reasoning: "effort", reasoningLevels: ["low", "high"], defaultReasoningLevel: "low" };
  return account;
}
function discovered(id = "vendor/discovered") {
  return { model: { ...customAccountFixture().custom!.models[0], id, name: "Modelo do catálogo", contextWindow: 1048576, maxOutputTokens: 131072, supportsImages: false, reasoning: "openrouter", reasoningLevels: ["off", "max", "high", "low"], defaultReasoningLevel: "high" }, sourceUrl: `https://openrouter.ai/${id}`, tokenField: "max_tokens" };
}
function deferred() {
  let resolve!: (value: ReturnType<typeof discovered>) => void;
  const promise = new Promise<ReturnType<typeof discovered>>(done => { resolve = done; });
  return { promise, resolve };
}
it("preenche pelo ID ao sair do campo, mostra skeleton e preserva chave e raciocínio escolhido", async () => {
  const user = userEvent.setup(); const lookup = deferred(); const account = openRouterAccount();
  invoke.mockImplementation((command: string) => command === "lookup_custom_model" ? lookup.promise : Promise.resolve(account));
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  expect(screen.queryByLabelText("Contexto (tokens) · modelo 1")).not.toBeInTheDocument();
  expect(screen.queryByRole("combobox", { name: "Campo de limite de saída" })).not.toBeInTheDocument();
  fireEvent.blur(screen.getByLabelText("ID do modelo 1"));
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("ID do modelo 1"), { target: { value: "vendor/discovered" } });
  fireEvent.blur(screen.getByLabelText("ID do modelo 1"));
  expect(screen.getByRole("status", { name: "Buscando dados do modelo 1" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeDisabled();
  expect(invoke).toHaveBeenCalledWith("lookup_custom_model", { baseUrl: account.custom!.baseUrl, modelId: "vendor/discovered", protocol: "openai-completions" });
  await act(async () => lookup.resolve(discovered()));
  expect(screen.getByText("1.048.576 contexto")).toBeInTheDocument();
  expect(screen.getByText("131.072 saída")).toBeInTheDocument();
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  expect(invoke).toHaveBeenCalledWith("save_custom_provider", expect.objectContaining({ apiKey: null, config: expect.objectContaining({ models: [expect.objectContaining({ id: "vendor/discovered", contextWindow: 1048576, maxOutputTokens: 131072, supportsImages: false, reasoning: "openrouter", defaultReasoningLevel: "low" })] }) }));
});
it.each(["id", "url", "endpoint"])("descarta resposta atrasada quando muda %s", async change => {
  const user = userEvent.setup(); const lookup = deferred(); invoke.mockReturnValue(lookup.promise);
  render(<CustomProviderForm account={openRouterAccount()} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Preencher modelo 1 pelo ID" }));
  if (change === "id") fireEvent.change(screen.getByLabelText("ID do modelo 1"), { target: { value: "outro/modelo" } });
  else if (change === "url") fireEvent.change(screen.getByLabelText("URL base"), { target: { value: "https://other.example/v1" } });
  else { await user.click(screen.getByRole("combobox", { name: "Endpoint" })); await user.click(await screen.findByRole("option", { name: "Anthropic · Messages" })); }
  await act(async () => lookup.resolve(discovered("vendor/model")));
  expect(screen.queryByText("1.048.576 contexto")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeEnabled();
});
it("não aplica metadados a outra linha ao remover um modelo durante a busca", async () => {
  const user = userEvent.setup(); const lookup = deferred(); invoke.mockReturnValue(lookup.promise);
  const account = openRouterAccount(); account.custom!.models.push({ ...account.custom!.models[0], id: "vendor/second", name: "Segundo" });
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 2" }));
  await user.click(screen.getByRole("button", { name: "Preencher modelo 2 pelo ID" }));
  await user.click(screen.getByRole("button", { name: "Remover modelo 1" }));
  await act(async () => lookup.resolve(discovered("vendor/second")));
  expect(screen.getByLabelText("ID do modelo 1")).toHaveValue("vendor/second");
  expect(screen.queryByText("1.048.576 contexto")).not.toBeInTheDocument();
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeEnabled();
});
it("abre configuração manual com ajuda quando o catálogo não encontra o modelo", async () => {
  const user = userEvent.setup(); invoke.mockRejectedValue({ message: "Modelo não encontrado. Copie o ID completo." });
  render(<CustomProviderForm account={openRouterAccount()} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Preencher modelo 1 pelo ID" }));
  expect(await screen.findByRole("alert", {}, { timeout: 1000 })).toHaveTextContent("Modelo não encontrado");
  expect(screen.getByLabelText("Contexto (tokens) · modelo 1")).toHaveValue(64000);
  await user.hover(screen.getByRole("button", { name: "Ajuda: Contexto (tokens) · modelo 1" }));
  expect(await screen.findByText(/Total que cabe na conversa/, {}, { timeout: 1000 })).toHaveTextContent("context_length");
  fireEvent.change(screen.getByLabelText("Contexto (tokens) · modelo 1"), { target: { value: "100000" } });
  expect(screen.getByLabelText("Saída máxima (tokens) · modelo 1")).toBeVisible();
});

it("recolhe modelos cadastrados e preserva edições ao alternar, adicionar e remover", async () => {
  const user = userEvent.setup(); const account = customAccountFixture();
  invoke.mockResolvedValue(account);
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.queryByLabelText("ID do modelo 1")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Configurar manualmente" }));
  fireEvent.change(screen.getByLabelText("Nome do modelo 1"), { target: { value: "Nome editado" } });
  await user.click(screen.getByRole("button", { name: "Adicionar modelo" }));
  expect(screen.queryByLabelText("ID do modelo 1")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("ID do modelo 2"), { target: { value: "vendor/draft" } });
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  expect(screen.getByLabelText("Nome do modelo 1")).toHaveValue("Nome editado");
  expect(screen.queryByLabelText("ID do modelo 2")).not.toBeInTheDocument();
  await user.keyboard("{Enter}");
  expect(screen.getByRole("button", { name: "Expandir modelo 1" })).toHaveAttribute("aria-expanded", "false");
  await user.click(screen.getByRole("button", { name: "Expandir modelo 2" }));
  expect(screen.getByLabelText("ID do modelo 2")).toHaveValue("vendor/draft");
  await user.click(screen.getByRole("button", { name: "Remover modelo 2" }));
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  expect(invoke).toHaveBeenCalledWith("save_custom_provider", expect.objectContaining({ apiKey: null, config: expect.objectContaining({ models: [expect.objectContaining({ name: "Nome editado" })] }) }));
});

it("mantém a busca ao recolher e salva os níveis Messages retornados pelo catálogo", async () => {
  const user = userEvent.setup(); const lookup = deferred(); const account = openRouterAccount();
  account.custom!.protocol = "anthropic-messages";
  account.custom!.models[0] = { ...account.custom!.models[0], reasoning: "none", reasoningLevels: [], defaultReasoningLevel: null };
  invoke.mockImplementation((command: string) => command === "lookup_custom_model" ? lookup.promise : Promise.resolve(account));
  render(<CustomProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  await user.click(screen.getByRole("button", { name: "Preencher modelo 1 pelo ID" }));
  await user.click(screen.getByRole("button", { name: "Recolher modelo 1" }));
  expect(screen.getByRole("status", { name: "Buscando dados do modelo 1" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeDisabled();
  const result = discovered("vendor/model");
  result.model = { ...result.model, reasoning: "adaptive", reasoningLevels: ["max", "high", "low"], defaultReasoningLevel: "max" };
  await act(async () => lookup.resolve(result));
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Salvar provedor" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Expandir modelo 1" })).toHaveAttribute("aria-expanded", "false");
  await user.click(screen.getByRole("button", { name: "Expandir modelo 1" }));
  expect(screen.getByText("Raciocínio · max / high / low")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Configurar manualmente" }));
  expect(screen.getByRole("combobox", { name: "Raciocínio · modelo 1" })).toHaveTextContent("Thinking · adaptativo");
  expect(screen.getByRole("combobox", { name: "Raciocínio padrão · modelo 1" })).toHaveTextContent("max");
  await user.click(screen.getByRole("button", { name: "Salvar provedor" }));
  expect(invoke).toHaveBeenCalledWith("save_custom_provider", expect.objectContaining({ config: expect.objectContaining({ protocol: "anthropic-messages", models: [expect.objectContaining({ reasoning: "adaptive", reasoningLevels: ["max", "high", "low"], defaultReasoningLevel: "max" })] }) }));
});
