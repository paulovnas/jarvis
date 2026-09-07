import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import { reasoningPreview } from "./reasoning-preview";

it("updates the live provider heading and preserves earlier reasoning in expandable history", async () => {
  const user = userEvent.setup();
  const first = { thinking: "**Analisando projeto**\n\nVou conferir as dependências.", commentary: "", tools: [] };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 2, steps: [first] }} />);
  expect(screen.getByRole("button", { name: /Analisando projeto/ })).toBeInTheDocument();
  rerender(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 3, steps: [first, { ...first, thinking: "**Conferindo testes**\n\nLendo o resultado." }] }} />);
  await user.click(screen.getByRole("button", { name: /Conferindo testes/ }));
  await user.click(screen.getByRole("button", { name: /Analisando projeto/ }));
  expect(screen.getByText(/Vou conferir as dependências/)).toBeVisible();
});
it("uses the latest paragraph when there is no heading and handles partial headings", () => {
  expect(reasoningPreview("Primeiro.\n\nSegundo." )).toBe("Segundo.");
  expect(reasoningPreview("**Primeiro**\nCorpo\n\n**Novo título")).toBe("Novo título");
});

it("shows every reasoning heading from the same provider step on first expansion", async () => {
  const user = userEvent.setup();
  render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 3, steps: [{ thinking: "**Analisando projeto**\n\nArquivos recebidos.\n\n**Conferindo testes**\n\nTestes encontrados.", commentary: "", tools: [] }] }} />);
  await user.click(screen.getByRole("button", { name: /Conferindo testes/ }));
  expect(screen.getByRole("button", { name: "Analisando projeto" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Conferindo testes" })).toBeVisible();
});
