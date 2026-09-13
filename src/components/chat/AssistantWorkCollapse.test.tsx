import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import { reasoningPreview } from "./reasoning-preview";

it("shows reconnection progress, its cause and returns to thinking after recovery", () => {
  const work = { durationSeconds: 3, steps: [{ thinking: "Conferindo os testes", commentary: "", tools: [] }], retry: { attempt: 1, maxAttempts: 5 as const, retryAt: 100, message: "HTTP 502 — Bad Gateway. O provedor está temporariamente indisponível." } };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={work} />);
  expect(screen.getByText("Reconectando 1/5")).toHaveAttribute("role", "status");
  expect(screen.getByText(work.retry.message)).toBeVisible();
  rerender(<AssistantWorkCollapse isStreaming work={{ ...work, retry: { ...work.retry, attempt: 5 } }} />);
  expect(screen.getByText("Reconectando 5/5")).toHaveAttribute("role", "status");
  rerender(<AssistantWorkCollapse isStreaming work={{ ...work, retry: null }} />);
  expect(screen.queryByText(/Reconectando/)).not.toBeInTheDocument();
  expect(screen.queryByText(work.retry.message)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /Conferindo os testes/ })).toBeVisible();
});

it("does not show a stale reconnection indicator on a finished turn", () => {
  render(<AssistantWorkCollapse work={{ durationSeconds: 10, steps: [], retry: { attempt: 5, maxAttempts: 5, retryAt: 0, message: "Falha de conexão" } }} />);
  expect(screen.getByRole("button", { name: /Trabalhou por 10s/ })).toBeInTheDocument();
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});

it("updates the live provider heading and preserves earlier reasoning on demand", async () => {
  const user = userEvent.setup();
  const first = { thinking: "**Analisando projeto**\n\nVou conferir as dependências.", commentary: "", tools: [] };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 2, steps: [first] }} />);
  expect(screen.getByRole("button", { name: /Analisando projeto/ })).toBeVisible();
  rerender(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 3, steps: [first, { ...first, thinking: "**Conferindo testes**\n\nLendo o resultado." }] }} />);
  await user.click(screen.getByRole("button", { name: /Analisou o contexto.*2 ações/ }));
  await user.click(screen.getByRole("button", { name: /Analisando projeto/ }));
  expect(screen.getByText(/Vou conferir as dependências/)).toBeVisible();
});

it("keeps live work open and folds it automatically when the turn finishes", () => {
  const work = { durationSeconds: 65, steps: [{ thinking: "Verificando o projeto", commentary: "", tools: [] }] };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={work} />);
  expect(screen.getByRole("button", { name: /Em execuçãoVerificando o projeto/ })).toHaveAttribute("aria-expanded", "true");
  rerender(<AssistantWorkCollapse work={work} />);
  expect(screen.getByRole("button", { name: /Trabalhou por 1m 05s/ })).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByRole("button", { name: /Analisou o contexto/ })).not.toBeInTheDocument();
});

it("summarizes long tool activity and mounts individual actions only on demand", async () => {
  const user = userEvent.setup();
  const names = ["ctx_search", "read", "bash", ...Array.from({ length: 17 }, () => "read")];
  const tools = names.map((name, index) => ({ id: `tool-${index}`, name, status: "completed" as const, args: {}, output: "" }));
  render(<AssistantWorkCollapse work={{ durationSeconds: 12, steps: [{ thinking: "", commentary: "", tools }] }} />);
  await user.click(screen.getByRole("button", { name: /Trabalhou por 12s/ }));
  const phase = screen.getByRole("button", { name: /Usou Context Mode, leu e pesquisou arquivos e executou comandos.*20 ações/ });
  expect(phase).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByTestId("tool-call-tool-0")).not.toBeInTheDocument();
  await user.click(phase);
  const firstBatch = screen.getByRole("button", { name: /Usou Context Mode, leu e pesquisou arquivos e executou comandos.*16 ações/ });
  expect(firstBatch).toHaveAttribute("aria-expanded", "false");
  await user.click(firstBatch);
  expect(screen.getByTestId("tool-call-tool-0")).toBeVisible();
  expect(screen.getAllByTestId(/tool-call-tool-/)).toHaveLength(16);
});

it("keeps an active activity summary closed while new actions arrive", () => {
  const first = Array.from({ length: 16 }, (_, index) => ({ id: `tool-${index}`, name: "read", status: (index === 15 ? "running" : "completed") as "running" | "completed", args: {}, output: "" }));
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 12, steps: [{ thinking: "", commentary: "", tools: first }] }} />);
  expect(screen.getByRole("button", { name: /Leu e pesquisou arquivos.*16 ações/ })).toHaveAttribute("aria-expanded", "false");
  const next = [...first.map(tool => ({ ...tool, status: "completed" as const })), { id: "tool-16", name: "ctx_search", status: "running" as const, args: {}, output: "" }];
  rerender(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 13, steps: [{ thinking: "", commentary: "", tools: next }] }} />);
  expect(screen.getByRole("button", { name: /Leu e pesquisou arquivos e usou Context Mode.*17 ações/ })).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByTestId("tool-call-tool-16")).not.toBeInTheDocument();
});

it("uses the latest paragraph when there is no heading and handles partial headings", () => {
  expect(reasoningPreview("Primeiro.\n\nSegundo." )).toBe("Segundo.");
  expect(reasoningPreview("**Primeiro**\nCorpo\n\n**Novo título")).toBe("Novo título");
});

it("keeps every reasoning heading hidden until the activity summary is expanded", async () => {
  const user = userEvent.setup();
  render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 3, steps: [{ thinking: "**Analisando projeto**\n\nArquivos recebidos.\n\n**Conferindo testes**\n\nTestes encontrados.", commentary: "", tools: [] }] }} />);
  expect(screen.queryByRole("list", { name: "Atividades da etapa 1" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Analisou o contexto.*2 ações/ }));
  const activity = screen.getByRole("list", { name: "Atividades da etapa 1" });
  expect(within(activity).getByRole("button", { name: /Analisando projeto/ })).toBeVisible();
  expect(within(activity).getByRole("button", { name: /Conferindo testes/ })).toBeVisible();
});

it("renders an observation as normal primary prose followed by one closed activity summary", async () => {
  const user = userEvent.setup();
  const tools = Array.from({ length: 5 }, (_, index) => ({ id: `read-${index}`, name: "read", status: "completed" as const, args: {}, output: "" }));
  render(<AssistantWorkCollapse work={{ durationSeconds: 8, steps: [{
    thinking: "**Investigando**\n\nProjeto carregado.\n\n**Comparando**\n\nContrato encontrado.\n\n**Validando**\n\nTestes localizados.\n\n**Concluindo**\n\nPronto para agir.",
    commentary: "Vou conferir esses arquivos antes de prosseguir.",
    tools,
  }] }} />);

  await user.click(screen.getByRole("button", { name: /Trabalhou por 8s/ }));
  const observation = screen.getByText("Vou conferir esses arquivos antes de prosseguir.");
  const observationRoot = observation.closest("[data-execution-observation]");
  const activity = screen.getByRole("button", { name: /Leu e pesquisou arquivos.*9 ações/ });
  expect(observationRoot).toHaveClass("text-sm", "text-foreground");
  expect(observationRoot?.querySelector(".italic")).toBeNull();
  expect(activity).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByRole("button", { name: /Raciocínio.*4 etapas/ })).not.toBeInTheDocument();
  expect(observation.compareDocumentPosition(activity) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
});

it("preserves the sequence of reasoning and tools inside an observation group", async () => {
  const user = userEvent.setup();
  const reads = Array.from({ length: 16 }, (_, index) => ({ id: `read-step-${index}`, name: "read", status: "completed" as const, args: {}, output: "" }));
  const commands = Array.from({ length: 5 }, (_, index) => ({ id: `bash-step-${index}`, name: "bash", status: "completed" as const, args: {}, output: "" }));
  render(<AssistantWorkCollapse work={{ durationSeconds: 11, steps: [
    { thinking: "Primeira análise", commentary: "Agora vou executar a verificação.", tools: reads },
    { thinking: "Resultado da verificação", commentary: "", tools: commands },
  ] }} />);

  await user.click(screen.getByRole("button", { name: /Trabalhou por 11s/ }));
  const observation = screen.getByText("Agora vou executar a verificação.");
  const phase = screen.getByRole("button", { name: /Leu e pesquisou arquivos e executou comandos.*23 ações/ });
  expect(observation.compareDocumentPosition(phase) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  await user.click(phase);
  const firstReasoning = screen.getByRole("button", { name: /Primeira análise/ });
  const readsGroup = screen.getByRole("button", { name: /Leu e pesquisou arquivos.*16 ações/ });
  const secondReasoning = screen.getByRole("button", { name: /Resultado da verificação/ });
  const commandsGroup = screen.getByRole("button", { name: /Executou comandos.*5 ações/ });
  expect(firstReasoning.compareDocumentPosition(readsGroup) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  expect(readsGroup.compareDocumentPosition(secondReasoning) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  expect(secondReasoning.compareDocumentPosition(commandsGroup) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
});

it("starts one closed activity group below each observation", async () => {
  const user = userEvent.setup();
  render(<AssistantWorkCollapse work={{ durationSeconds: 9, steps: [
    { thinking: "Analisando o pedido", commentary: "Vou conferir a implementação.", tools: [{ id: "read-one", name: "read", status: "completed", args: { path: "src/App.tsx" }, output: "" }] },
    { thinking: "Validando o resultado", commentary: "Agora vou executar os testes.", tools: [{ id: "test-one", name: "bash", status: "completed", args: { command: "bun test" }, output: "ok" }] },
  ] }} />);

  await user.click(screen.getByRole("button", { name: /Trabalhou por 9s/ }));
  const phases = within(screen.getByRole("list", { name: "Etapas da execução" }))
    .getAllByRole("listitem")
    .filter(item => item.hasAttribute("data-execution-phase"));
  expect(phases).toHaveLength(2);
  expect(within(phases[0]).getByText("Vou conferir a implementação.")).toBeVisible();
  expect(within(phases[1]).getByText("Agora vou executar os testes.")).toBeVisible();
  const firstActivity = within(phases[0]).getByRole("button", { name: /Leu e pesquisou arquivos.*2 ações/ });
  const secondActivity = within(phases[1]).getByRole("button", { name: /Executou comandos.*2 ações/ });
  expect(firstActivity).toHaveAttribute("aria-expanded", "false");
  expect(secondActivity).toHaveAttribute("aria-expanded", "false");
  await user.click(firstActivity);
  expect(within(phases[0]).getAllByRole("listitem").map(item => item.dataset.activityKind)).toEqual(["reasoning", "tool"]);
  expect(screen.queryByText(/^0[1-9]$/)).not.toBeInTheDocument();
});

it("keeps a streaming observation stable while its closed activity summary updates", () => {
  const observation = "Vou inspecionar os arquivos relevantes.";
  const initial = { durationSeconds: 2, steps: [{ thinking: "Localizando arquivos", commentary: observation, tools: [] }] };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={initial} />);
  const phase = screen.getByText(observation).closest("[data-execution-phase]");

  rerender(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 3, steps: [{
    ...initial.steps[0],
    tools: [{ id: "live-read", name: "read", status: "running", args: { path: "src/App.tsx" }, output: "" }],
  }] }} />);

  const currentObservation = screen.getByText(observation);
  const activity = screen.getByRole("button", { name: /Leu e pesquisou arquivos.*2 ações/ });
  expect(currentObservation.closest("[data-execution-phase]")).toBe(phase);
  expect(activity).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByTestId("tool-call-live-read")).not.toBeInTheDocument();
  expect(currentObservation.compareDocumentPosition(activity) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
});

it("starts a new closed group when a later observation arrives", () => {
  const activeTools = Array.from({ length: 5 }, (_, index) => ({
    id: `phase-read-${index}`,
    name: "read",
    status: (index === 4 ? "running" : "completed") as "running" | "completed",
    args: {},
    output: "",
  }));
  const first = { thinking: "Analisando arquivos", commentary: "Vou mapear a implementação.", tools: activeTools };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 4, steps: [first] }} />);
  expect(screen.getByRole("button", { name: /Leu e pesquisou arquivos.*6 ações/ })).toHaveAttribute("aria-expanded", "false");

  rerender(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 5, steps: [
    { ...first, tools: activeTools.map(tool => ({ ...tool, status: "completed" as const })) },
    { thinking: "Validando resultado", commentary: "Agora vou validar a alteração.", tools: [] },
  ] }} />);

  const phases = within(screen.getByRole("list", { name: "Etapas da execução" }))
    .getAllByRole("listitem")
    .filter(item => item.hasAttribute("data-execution-phase"));
  expect(phases).toHaveLength(2);
  expect(within(phases[1]).getByText("Agora vou validar a alteração.")).toBeVisible();
  expect(within(phases[0]).getByRole("button", { name: /Leu e pesquisou arquivos.*6 ações/ })).toHaveAttribute("aria-expanded", "false");
  expect(within(phases[1]).getByRole("button", { name: /Analisou o contexto.*1 ação/ })).toHaveAttribute("aria-expanded", "false");
});

it("does not reorder actions around a later reasoning step", async () => {
  const user = userEvent.setup();
  const reads = Array.from({ length: 3 }, (_, index) => ({ id: `read-${index}`, name: "read", status: "completed" as const, args: {}, output: "" }));
  const commands = Array.from({ length: 3 }, (_, index) => ({ id: `bash-${index}`, name: "bash", status: "completed" as const, args: {}, output: "" }));
  render(<AssistantWorkCollapse work={{ durationSeconds: 6, steps: [
    { thinking: "Primeira leitura", commentary: "", tools: reads },
    { thinking: "Agora vou validar", commentary: "", tools: commands },
  ] }} />);

  await user.click(screen.getByRole("button", { name: /Trabalhou por 6s/ }));
  await user.click(screen.getByRole("button", { name: /Leu e pesquisou arquivos e executou comandos.*8 ações/ }));
  const lastRead = screen.getByTestId("tool-call-read-2");
  const laterReasoning = screen.getByRole("button", { name: /Agora vou validar/ });
  const firstCommand = screen.getByTestId("tool-call-bash-0");
  expect(lastRead.compareDocumentPosition(laterReasoning) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  expect(laterReasoning.compareDocumentPosition(firstCommand) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
});
