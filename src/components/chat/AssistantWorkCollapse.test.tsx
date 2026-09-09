import { render, screen } from "@testing-library/react";
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
it("uses the latest paragraph when there is no heading and handles partial headings", () => {
  expect(reasoningPreview("Primeiro.\n\nSegundo." )).toBe("Segundo.");
  expect(reasoningPreview("**Primeiro**\nCorpo\n\n**Novo título")).toBe("Novo título");
});

it("shows every reasoning heading from the same provider step on first expansion", async () => {
  render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 3, steps: [{ thinking: "**Analisando projeto**\n\nArquivos recebidos.\n\n**Conferindo testes**\n\nTestes encontrados.", commentary: "", tools: [] }] }} />);
  expect(screen.getByRole("button", { name: "Analisando projeto" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Conferindo testes" })).toBeVisible();
});
