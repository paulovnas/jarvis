import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { PendingApproval } from "@/core/chat";
import { ToolApproval } from "./ToolApproval";

const commandRequest = (overrides?: Partial<NonNullable<PendingApproval["policy"]>>): PendingApproval => ({
  tool: {
    id: "bash-1",
    name: "bash",
    args: { command: "git push origin feature" },
    status: "pending",
    output: "",
    durationMs: 0,
  },
  policy: {
    code: "network_requires_approval",
    reason: "O comando acessa a rede e altera o repositório.",
    effects: { readsFilesystem: true, writesFilesystem: true, usesNetwork: true, controlsProcesses: false, destructive: false, dynamic: false, unknown: false },
    command: { invocations: [{ argv: ["git", "push", "origin", "feature"] }], redirections: [], dynamic: false },
    readPaths: ["/projeto"],
    writePaths: ["/projeto/.git"],
    workingDirectory: "/projeto",
    repositoryRoot: "/projeto",
    commandPrefixAvailable: true,
    sandbox: { backend: "macosSeatbelt", availability: "full", filesystemIsolated: true, network: "allowed", processTreeIsolated: true, reason: null },
    ...overrides,
  },
});

it("explains and submits a one-time approval for a terminal owned by another context", async () => {
  const user = userEvent.setup();
  const answer = vi.fn(async () => true);
  render(<ToolApproval
    request={{
      tool: { id: "close-1", name: "terminal_close", args: { id: "terminal-user", reason: "A verificação terminou." }, status: "pending", output: "", durationMs: 0 },
      policy: null,
    }}
    projectPath="/projeto"
    onAnswer={answer}
  />);

  expect(screen.getByText("Autorizar fechamento de terminal?")).toBeVisible();
  expect(screen.getByText("Terminal")).toBeVisible();
  expect(screen.getByText("terminal-user")).toBeVisible();
  expect(screen.getByText("Motivo")).toBeVisible();
  expect(screen.getByText("A verificação terminou.")).toBeVisible();

  await user.click(screen.getByRole("button", { name: "Autorizar uma vez" }));
  expect(answer).toHaveBeenCalledWith({ approved: true, grant: null });
});

it("shows interpreted effects and creates a persistent project grant", async () => {
  const user = userEvent.setup();
  const answer = vi.fn(async () => true);
  render(<ToolApproval request={commandRequest()} projectPath="/projeto" onAnswer={answer} />);

  expect(screen.getByText("git push origin feature")).toBeVisible();
  expect(screen.getByText("Escreve arquivos")).toBeVisible();
  expect(screen.getByText("Usa rede")).toBeVisible();
  expect(screen.getByText(/Seatbelt do macOS/)).toBeVisible();

  await user.click(screen.getByRole("switch", { name: "Lembrar esta autorização" }));
  await user.click(screen.getByRole("combobox", { name: "Escopo da autorização" }));
  await user.click(screen.getByRole("option", { name: "Este projeto" }));
  await waitFor(() => expect(screen.getByRole("combobox", { name: "Escopo da autorização" })).toHaveTextContent("Este projeto"));
  await user.click(screen.getByRole("combobox", { name: "Duração da autorização" }));
  await user.click(await screen.findByRole("option", { name: "Manter entre sessões" }));
  await user.click(screen.getByRole("button", { name: "Autorizar e lembrar" }));

  expect(answer).toHaveBeenCalledWith({ approved: true, grant: { scope: "project", duration: "persistent", matchKind: "exact" } });
});

it("keeps repository and command-prefix grants unavailable when the policy cannot prove them", async () => {
  const user = userEvent.setup();
  render(<ToolApproval request={commandRequest({ repositoryRoot: null, commandPrefixAvailable: false })} projectPath="/projeto" onAnswer={vi.fn(async () => true)} />);

  await user.click(screen.getByRole("switch", { name: "Lembrar esta autorização" }));
  await screen.findByRole("combobox", { name: "Escopo da autorização" });
  await user.click(screen.getByRole("combobox", { name: "Escopo da autorização" }));
  expect(await screen.findByRole("option", { name: "Este repositório" })).toHaveAttribute("aria-disabled", "true");
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("combobox", { name: "Correspondência da autorização" }));
  expect(await screen.findByRole("option", { name: "Comandos com o mesmo prefixo" })).toHaveAttribute("aria-disabled", "true");
});
