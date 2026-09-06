import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ProviderAccount } from "@/core/provider-accounts";
import { customAccountFixture } from "@/test/custom-provider-fixtures";

it("mostra configuração Custom sem simular conexão ou cotas e permite editar", async () => {
  const account = customAccountFixture(); const edit = vi.fn(); const user = userEvent.setup();
  render(<ProviderAccountCard account={account} onEdit={edit} onDisconnect={vi.fn()} onEnabledChange={vi.fn()} />);
  expect(screen.getByText("Configurada")).toBeVisible();
  await user.click(screen.getByRole("button", { name: `Detalhes de ${account.alias}` }));
  expect(screen.getByText(account.custom!.baseUrl)).toBeVisible();
  expect(screen.queryByRole("switch", { name: /Limites/ })).not.toBeInTheDocument();
  expect(screen.queryByText("E-mail")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Editar" }));
  expect(edit).toHaveBeenCalledWith(account);
});
import { ProviderAccountCard } from "./ProviderAccountCard";

const account: ProviderAccount = {
  alias: "openai-codex-pessoal", providerKind: "openai-codex", enabled: true, createdAt: 1_735_689_600,
  email: "dev@example.test", accountType: "personal", modelsAvailable: true,
  models: [{ id: "test-model", name: "Modelo de teste", reasoningLevels: [], defaultReasoningLevel: null }],
};

describe("ProviderAccountCard", () => {
  it("desativa e reativa sem desconectar a conta", async () => {
    const user = userEvent.setup();
    const onEnabledChange = vi.fn();
    const onDisconnect = vi.fn();
    const view = render(<ProviderAccountCard account={account} onDisconnect={onDisconnect} onEnabledChange={onEnabledChange} />);
    await user.click(screen.getByRole("button", { name: `Detalhes de ${account.alias}` }));
    await user.click(screen.getByRole("switch", { name: `Ativar ${account.alias}` }));
    expect(onEnabledChange).toHaveBeenCalledWith(account.alias, false);
    view.rerender(<ProviderAccountCard account={{ ...account, enabled: false }} onDisconnect={onDisconnect} onEnabledChange={onEnabledChange} />);
    expect(screen.getByText("Ative a conta para disponibilizar seus modelos.")).toBeVisible();
    await user.click(screen.getByRole("switch", { name: `Ativar ${account.alias}` }));
    expect(onEnabledChange).toHaveBeenLastCalledWith(account.alias, true);
    expect(onDisconnect).not.toHaveBeenCalled();
  });
  it("abre os detalhes em uma modal pelo teclado e restaura o foco ao fechar", async () => {
    const user = userEvent.setup();
    const onDisconnect = vi.fn();
    render(<ProviderAccountCard account={account} onDisconnect={onDisconnect} onEnabledChange={vi.fn()} />);
    const trigger = screen.getByRole("button", { name: `Detalhes de ${account.alias}` });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("OpenAI Codex · 1 modelo")).toBeVisible();
    expect(screen.queryByText(account.email!)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Desconectar" })).not.toBeInTheDocument();

    await user.tab();
    expect(trigger).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText(account.email!)).toBeVisible();
    expect(screen.getByText("Modelo de teste")).toBeVisible();
    expect(screen.getByRole("dialog", { name: account.alias })).toBeVisible();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Desconectar" })).not.toBeInTheDocument());
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveFocus();
    expect(onDisconnect).not.toHaveBeenCalled();
  });

  it("mantém indisponibilidade de modelos visível no resumo", async () => {
    const user = userEvent.setup();
    render(<ProviderAccountCard account={{ ...account, modelsAvailable: false, models: [] }} onDisconnect={vi.fn()} onEnabledChange={vi.fn()} />);
    expect(screen.getByText("OpenAI Codex · Modelos indisponíveis")).toBeVisible();
    await user.click(screen.getByRole("button"));
    expect(screen.getByText("Não foi possível consultar os modelos agora.")).toBeVisible();
  });

  it("distingue uma lista vazia de modelos e preserva os dados ausentes", async () => {
    const user = userEvent.setup();
    const onDisconnect = vi.fn();
    render(<ProviderAccountCard account={{ ...account, email: null, accountType: "unknown", createdAt: 0, models: [] }} onDisconnect={onDisconnect} onEnabledChange={vi.fn()} />);
    expect(screen.getByText("OpenAI Codex · Nenhum modelo")).toBeVisible();
    await user.click(screen.getByRole("button"));
    expect(screen.getByText("Não informado")).toBeVisible();
    expect(screen.getByText("Não identificado")).toBeVisible();
    expect(screen.getByText("Data indisponível")).toBeVisible();
    expect(screen.getByText("A assinatura não retornou modelos.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Desconectar" }));
    expect(onDisconnect).toHaveBeenCalledExactlyOnceWith(account.alias);
  });
});
