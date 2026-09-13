import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import { reasoningPreview } from "./reasoning-preview";

it("shows reconnection progress, expandable cause and returns to thinking after recovery", async () => {
  const work = { durationSeconds: 3, steps: [{ thinking: "Conferindo os testes", commentary: "", tools: [] }], retry: { attempt: 1, maxAttempts: 5 as const, retryAt: 100, message: "HTTP 502 — Bad Gateway. O provedor está temporariamente indisponível." } };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={work} />);
  expect(screen.getByRole("status")).toHaveTextContent("Reconectando 1/5");
  expect(screen.getByText(work.retry.message)).toBeVisible();
  rerender(<AssistantWorkCollapse isStreaming work={{ ...work, retry: { ...work.retry, attempt: 5 } }} />);
  expect(screen.getByRole("status")).toHaveTextContent("Reconectando 5/5");
  rerender(<AssistantWorkCollapse isStreaming work={{ ...work, retry: null }} />);
  expect(screen.queryByText(/Reconectando/)).not.toBeInTheDocument();
  expect(screen.queryByText(work.retry.message)).not.toBeInTheDocument();
  expect(screen.getAllByRole("button", { name: /Conferindo os testes/ }).length).toBeGreaterThan(0);
});

it("does not show a stale reconnection indicator on a finished turn", () => {
  render(<AssistantWorkCollapse work={{ durationSeconds: 10, steps: [], retry: { attempt: 5, maxAttempts: 5, retryAt: 0, message: "Falha de conexão" } }} />);
  expect(screen.getByRole("button", { name: /Trabalhou por 10s/ })).toBeInTheDocument();
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});

it("updates the live provider heading and preserves earlier reasoning in expandable history", async () => {
  const user = userEvent.setup();
  const first = { thinking: "**Analisando projeto**\n\nVou conferir as dependências.", commentary: "", tools: [] };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 2, steps: [first] }} />);
  expect(screen.getAllByRole("button", { name: /Analisando projeto/ })).toHaveLength(2);
  rerender(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 3, steps: [first, { ...first, thinking: "**Conferindo testes**\n\nLendo o resultado." }] }} />);
  await user.click(screen.getByRole("button", { name: /Analisando projeto/ }));
  expect(screen.getByText(/Vou conferir as dependências/)).toBeVisible();
});

it("keeps live work open and folds it automatically when the turn finishes", () => {
  const work = { durationSeconds: 65, steps: [{ thinking: "Verificando o projeto", commentary: "", tools: [] }] };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={work} />);
  expect(screen.getByRole("button", { name: /Em execuçãoVerificando o projeto/ })).toHaveAttribute("aria-expanded", "true");
  rerender(<AssistantWorkCollapse work={work} />);
  expect(screen.getByRole("button", { name: /Trabalhou por 1m 05s/ })).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByRole("button", { name: "Verificando o projeto" })).not.toBeInTheDocument();
});

it("summarizes long tool activity and mounts individual actions only on demand", async () => {
  const user = userEvent.setup();
  const names = ["ctx_search", "read", "bash", ...Array.from({ length: 17 }, () => "read")];
  const tools = names.map((name, index) => ({ id: `tool-${index}`, name, status: "completed" as const, args: {}, output: "" }));
  render(<AssistantWorkCollapse work={{ durationSeconds: 12, steps: [{ thinking: "", commentary: "", tools }] }} />);
  await user.click(screen.getByRole("button", { name: /Trabalhou por 12s/ }));
  expect(screen.getByRole("button", { name: /Usou Context Mode, leu e pesquisou arquivos e executou comandos.*16 ações/ })).toBeVisible();
  expect(screen.getAllByRole("button", { name: /Leu e pesquisou arquivos/ })).toHaveLength(1);
  expect(screen.queryByTestId("tool-call-tool-0")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Usou Context Mode, leu e pesquisou arquivos e executou comandos.*16 ações/ }));
  expect(screen.getByTestId("tool-call-tool-0")).toBeVisible();
  expect(screen.getAllByTestId(/tool-call-tool-/)).toHaveLength(16);
});
it("folds a completed activity group when a new group starts", () => {
  const first = Array.from({ length: 16 }, (_, index) => ({ id: `tool-${index}`, name: "read", status: (index === 15 ? "running" : "completed") as "running" | "completed", args: {}, output: "" }));
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 12, steps: [{ thinking: "", commentary: "", tools: first }] }} />);
  expect(screen.getByRole("button", { name: /Leu e pesquisou arquivos.*16 ações/ })).toHaveAttribute("aria-expanded", "true");
  const next = [...first.map(tool => ({ ...tool, status: "completed" as const })), { id: "tool-16", name: "ctx_search", status: "running" as const, args: {}, output: "" }];
  rerender(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 13, steps: [{ thinking: "", commentary: "", tools: next }] }} />);
  expect(screen.getByRole("button", { name: /Leu e pesquisou arquivos.*16 ações/ })).toHaveAttribute("aria-expanded", "false");
  expect(screen.getByRole("button", { name: /Usou Context Mode.*1 ação/ })).toHaveAttribute("aria-expanded", "true");
});
it("uses the latest paragraph when there is no heading and handles partial headings", () => {
  expect(reasoningPreview("Primeiro.\n\nSegundo." )).toBe("Segundo.");
  expect(reasoningPreview("**Primeiro**\nCorpo\n\n**Novo título")).toBe("Novo título");
});

it("shows every reasoning heading from the same provider step on first expansion", async () => {
  render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 3, steps: [{ thinking: "**Analisando projeto**\n\nArquivos recebidos.\n\n**Conferindo testes**\n\nTestes encontrados.", commentary: "", tools: [] }] }} />);
  const timeline = screen.getByRole("list", { name: "Linha do tempo da execução" });
  expect(within(timeline).getByRole("button", { name: /Analisando projeto/ })).toBeVisible();
  expect(within(timeline).getByRole("button", { name: /Conferindo testes/ })).toBeVisible();
});

it("keeps observations outside reasoning as italic chronological text", async () => {
  const user = userEvent.setup();
  const tools = Array.from({ length: 5 }, (_, index) => ({ id: `read-${index}`, name: "read", status: "completed" as const, args: {}, output: "" }));
  render(<AssistantWorkCollapse work={{ durationSeconds: 8, steps: [{
    thinking: "**Investigando**\n\nProjeto carregado.\n\n**Comparando**\n\nContrato encontrado.\n\n**Validando**\n\nTestes localizados.\n\n**Concluindo**\n\nPronto para agir.",
    commentary: "Vou conferir esses arquivos antes de prosseguir.",
    tools,
  }] }} />);

  await user.click(screen.getByRole("button", { name: /Trabalhou por 8s/ }));
  const reasoning = screen.getByRole("button", { name: /Raciocínio.*4 etapas/ });
  const observation = screen.getByText("Vou conferir esses arquivos antes de prosseguir.");
  const activity = screen.getByRole("button", { name: /Leu e pesquisou arquivos.*5 ações/ });

  expect(reasoning).toBeVisible();
  expect(observation).toHaveClass("italic");
  expect(screen.queryByRole("button", { name: /Observações/ })).not.toBeInTheDocument();
  expect(reasoning.compareDocumentPosition(observation) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  expect(observation.compareDocumentPosition(activity) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
});

it("preserves the sequence between observations, actions and later reasoning", async () => {
  const user = userEvent.setup();
  const reads = Array.from({ length: 16 }, (_, index) => ({ id: `read-step-${index}`, name: "read", status: "completed" as const, args: {}, output: "" }));
  const commands = Array.from({ length: 5 }, (_, index) => ({ id: `bash-step-${index}`, name: "bash", status: "completed" as const, args: {}, output: "" }));
  render(<AssistantWorkCollapse work={{ durationSeconds: 11, steps: [
    { thinking: "Primeira análise", commentary: "Agora vou executar a verificação.", tools: reads },
    { thinking: "Resultado da verificação", commentary: "", tools: commands },
  ] }} />);

  await user.click(screen.getByRole("button", { name: /Trabalhou por 11s/ }));
  const firstReasoning = screen.getByRole("button", { name: /Primeira análise/ });
  const observation = screen.getByText("Agora vou executar a verificação.");
  const readsGroup = screen.getByRole("button", { name: /Leu e pesquisou arquivos.*16 ações/ });
  const secondReasoning = screen.getByRole("button", { name: /Resultado da verificação/ });

  expect(firstReasoning.compareDocumentPosition(observation) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  expect(observation.compareDocumentPosition(readsGroup) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  expect(readsGroup.compareDocumentPosition(secondReasoning) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
});

it("presents reasoning, observations and actions on one numbered chronological timeline", async () => {
  const user = userEvent.setup();
  render(<AssistantWorkCollapse work={{ durationSeconds: 9, steps: [
    { thinking: "Analisando o pedido", commentary: "Vou conferir a implementação.", tools: [{ id: "read-one", name: "read", status: "completed", args: { path: "src/App.tsx" }, output: "" }] },
    { thinking: "Validando o resultado", commentary: "", tools: [{ id: "test-one", name: "bash", status: "completed", args: { command: "bun test" }, output: "ok" }] },
  ] }} />);

  await user.click(screen.getByRole("button", { name: /Trabalhou por 9s/ }));
  const timeline = screen.getByRole("list", { name: "Linha do tempo da execução" });
  const rows = within(timeline).getAllByRole("listitem");

  expect(rows.map(row => row.dataset.timelineKind)).toEqual(["reasoning", "commentary", "tool", "reasoning", "tool"]);
  expect(rows.map(row => within(row).getByText(/^\d{2}$/).textContent)).toEqual(["01", "02", "03", "04", "05"]);
});

it("does not group actions across a later reasoning step", async () => {
  const user = userEvent.setup();
  const reads = Array.from({ length: 3 }, (_, index) => ({ id: `read-${index}`, name: "read", status: "completed" as const, args: {}, output: "" }));
  const commands = Array.from({ length: 3 }, (_, index) => ({ id: `bash-${index}`, name: "bash", status: "completed" as const, args: {}, output: "" }));
  render(<AssistantWorkCollapse work={{ durationSeconds: 6, steps: [
    { thinking: "Primeira leitura", commentary: "", tools: reads },
    { thinking: "Agora vou validar", commentary: "", tools: commands },
  ] }} />);

  await user.click(screen.getByRole("button", { name: /Trabalhou por 6s/ }));
  const lastRead = screen.getByTestId("tool-call-read-2");
  const laterReasoning = screen.getByRole("button", { name: /Agora vou validar/ });
  const firstCommand = screen.getByTestId("tool-call-bash-0");

  const timeline = screen.getByRole("list", { name: "Linha do tempo da execução" });
  expect(within(timeline).queryByRole("button", { name: /6 ações/ })).not.toBeInTheDocument();
  expect(lastRead.compareDocumentPosition(laterReasoning) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  expect(laterReasoning.compareDocumentPosition(firstCommand) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
});
