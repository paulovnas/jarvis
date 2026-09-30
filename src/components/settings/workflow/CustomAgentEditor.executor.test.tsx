import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { markdownSource } from "@/test/markdown-editor";
import { expect, it, vi } from "vitest";
import { CustomAgentEditor } from "./CustomAgentEditor";
import { customAgent } from "@/test/workflow-fixtures";
import type { CustomAgent } from "@/core/workflow-catalog";
import type { ProviderAccount } from "@/core/provider-accounts";

vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => ({ data: { installed: false, authenticated: false, version: null, models: [], error: null }, loading: false, error: null, refresh: vi.fn() }) }));

const accounts: ProviderAccount[] = ["primary", "backup"].map(alias => ({ alias, providerKind: "openai-codex", enabled: true, createdAt: 0, email: null, accountType: "personal", modelsAvailable: true, models: [{ id: "model", name: "Modelo de teste", reasoningLevels: [], defaultReasoningLevel: null }] }));

it.each(["solo", "mixed", "flow_only"] as const)("saves, restores and removes the secondary model for a %s custom agent", async usage => {
  const user = userEvent.setup(); const save = vi.fn().mockResolvedValue(true);
  const initial: CustomAgent = { ...customAgent, usage, instructions: "Instruções extensas do agente.\n".repeat(200), model: { account: "primary", model: "model", reasoning: null } };
  const props = { accounts, saving: false, creating: false, onSave: save, onClose: vi.fn() };
  const first = render(<CustomAgentEditor {...props} initial={initial} />);
  const name = "Modelo secundário do agente customizado";
  const settings = within(screen.getByRole("group", { name: "Identidade e modelo" }));
  expect(settings.getByRole("textbox", { name: "Nome" })).toHaveValue(initial.name);
  expect(settings.getByRole("button", { name: "Modelo do agente customizado" })).toBeVisible();
  expect(settings.getByRole("button", { name })).toBeVisible();
  expect(await markdownSource(user, "Instruções do agente")).toHaveValue(initial.instructions);
  expect(screen.getByRole("button", { name })).toHaveTextContent("Nenhum");
  await user.click(screen.getByRole("button", { name }));
  (await screen.findByRole("menuitem", { name: "backup" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Modelo de teste" }));
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  const saved = { ...initial, model: { ...initial.model!, fallback: { executor: "jarvis" as const, account: "backup", model: "model", reasoning: null } } };
  await waitFor(() => expect(save).toHaveBeenLastCalledWith(saved));
  first.unmount();
  render(<CustomAgentEditor {...props} initial={saved} />);
  expect(screen.getByRole("button", { name })).toHaveTextContent("backup · Modelo de teste");
  await user.click(screen.getByRole("button", { name }));
  await user.click(await screen.findByRole("menuitem", { name: "Nenhum" }));
  expect(screen.getByRole("button", { name })).toHaveTextContent("Nenhum");
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  await waitFor(() => expect(save).toHaveBeenLastCalledWith({ ...initial, model: { ...initial.model!, fallback: null } }));
});

it.each(["primary", "secondary"] as const)("swaps, saves and restores the custom agent model assignments when selecting the other target as %s", async slot => {
  const user = userEvent.setup(); const save = vi.fn().mockResolvedValue(true);
  const primary = { executor: "jarvis" as const, account: "primary", model: "model", reasoning: "high" };
  const fallback = { ...primary, account: "backup", reasoning: "xhigh" };
  const initial = { ...customAgent, model: { ...primary, fallback } };
  const providerAccounts = accounts.map(account => ({ ...account, models: account.models.map(model => ({ ...model, reasoningLevels: ["high", "xhigh"], defaultReasoningLevel: "high" })) }));
  const props = { accounts: providerAccounts, saving: false, creating: false, onSave: save, onClose: vi.fn() };
  const first = render(<CustomAgentEditor initial={initial} {...props} />);
  const primaryName = "Modelo do agente customizado";
  const secondaryName = "Modelo secundário do agente customizado";
  await user.click(screen.getByRole("button", { name: slot === "primary" ? primaryName : secondaryName }));
  (await screen.findByRole("menuitem", { name: slot === "primary" ? "backup" : "primary" })).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem", { name: "Modelo de teste" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Salvar agente" })).toBeEnabled();
  expect(screen.getByRole("button", { name: primaryName })).toHaveTextContent("Extra alto");
  expect(screen.getByRole("button", { name: secondaryName })).toHaveTextContent("primary · Modelo de teste · Alto");
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  const saved = { ...initial, model: { ...fallback, fallback: primary } };
  await waitFor(() => expect(save).toHaveBeenCalledExactlyOnceWith(saved));
  first.unmount();
  render(<CustomAgentEditor initial={saved} {...props} />);
  expect(screen.getByRole("button", { name: primaryName })).toHaveTextContent("Extra alto");
  expect(screen.getByRole("button", { name: secondaryName })).toHaveTextContent("primary · Modelo de teste · Alto");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("saves a custom Claude agent before CLI setup without requiring a Jarvis provider", async () => {
  const user = userEvent.setup(); const save = vi.fn().mockResolvedValue(true); const close = vi.fn();
  render(<CustomAgentEditor initial={customAgent} accounts={[]} saving={false} creating={false} onSave={save} onClose={close} />);
  await user.click(screen.getByRole("button", { name: "Modelo do agente customizado" }));
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Padrão do Claude Code" }));
  expect(screen.getByText("Modelo fixo: Claude Code / default")).toBeVisible();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  await waitFor(() => expect(save).toHaveBeenCalledExactlyOnceWith({ ...customAgent, model: { executor: "claude", account: "", model: "default", reasoning: null } }));
  expect(close).toHaveBeenCalledOnce();
});
