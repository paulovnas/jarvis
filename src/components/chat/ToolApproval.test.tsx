import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { computerSnapshot, computerTarget } from "@/test/computer-fixtures";
import type { PendingApproval } from "@/core/chat";
import { ToolApproval } from "./ToolApproval";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset().mockResolvedValue(computerSnapshot({ targets: [computerTarget] })); });

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

it("reviews native input once without offering a broader execution grant", async () => {
  const user = userEvent.setup();
  const onAnswer = vi.fn().mockResolvedValue(true);
  const request = commandRequest();
  request.tool = { ...request.tool, name: "computer_click", args: { targetId: "safari-window", observationId: "observation-1", element: "submit" } };
  render(<ToolApproval request={request} projectPath="/projeto" conversationId="c1" onAnswer={onAnswer} />);
  expect(await screen.findByText("Safari")).toBeVisible();
  expect(screen.getByText("Documentação")).toBeVisible();
  expect(screen.getByText("Autorizar interação no aplicativo?")).toBeVisible();
  expect(screen.getByText("Alvo autorizado")).toBeVisible();
  expect(screen.getAllByText("safari-window")).toHaveLength(2);
  expect(screen.queryByRole("switch", { name: "Lembrar esta autorização" })).not.toBeInTheDocument();
  expect(screen.queryByRole("combobox", { name: "Escopo da autorização" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Autorizar uma vez" }));
  await waitFor(() => expect(onAnswer).toHaveBeenCalledWith({ approved: true, grant: null }));
});

it("keeps a refused native input action unapproved", async () => {
  const user = userEvent.setup();
  const onAnswer = vi.fn().mockResolvedValue(true);
  const request = commandRequest();
  request.tool = { ...request.tool, name: "computer_type", args: { targetId: "safari-window", observationId: "observation-1", text: "Texto proposto" } };
  render(<ToolApproval request={request} projectPath="/projeto" conversationId="c1" onAnswer={onAnswer} />);
  expect(screen.getByText("Texto proposto")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Recusar" }));
  expect(onAnswer).toHaveBeenCalledWith({ approved: false, grant: null });
});

it("does not approve a native action when its verified target has disappeared", async () => {
  const user = userEvent.setup();
  const onAnswer = vi.fn().mockResolvedValue(true);
  vi.mocked(invoke).mockResolvedValue(computerSnapshot());
  const request = commandRequest();
  request.tool = { ...request.tool, name: "computer_press", args: { targetId: "missing", observationId: "observation-1", key: "Enter", appName: "Untrusted Safari" } };
  render(<ToolApproval request={request} projectPath="/projeto" conversationId="c1" onAnswer={onAnswer} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("O alvo desta ação não está disponível para revisão.");
  expect(screen.getByRole("button", { name: "Autorizar uma vez" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Recusar" }));
  expect(onAnswer).toHaveBeenCalledWith({ approved: false, grant: null });
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
  await user.click(await screen.findByRole("option", { name: "Este projeto" }));
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

it("shows network access before authorizing a project test command", async () => {
  const user = userEvent.setup();
  const answer = vi.fn(async () => true);
  const request = commandRequest({
    code: "network_approval_required",
    reason: "O comando pode usar rede, incluindo serviços locais, testes e servidores de desenvolvimento.",
    command: { invocations: [{ argv: ["npm", "test"] }], redirections: [], dynamic: false },
    writePaths: ["/projeto"],
  });
  request.tool.args = { command: "npm test" };
  render(<ToolApproval request={request} projectPath="/projeto" onAnswer={answer} />);

  expect(screen.getByText("npm test")).toBeVisible();
  expect(screen.getByText("Usa rede")).toBeVisible();
  expect(screen.getByText(/Rede: permitida/)).toBeVisible();
  expect(screen.getByText(/incluindo serviços locais/)).toBeVisible();
  expect(answer).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Autorizar uma vez" }));
  expect(answer).toHaveBeenCalledWith({ approved: true, grant: null });
});

it("shows the actual video tool, provider, model and estimate before approval", async () => {
  const user = userEvent.setup();
  const answer = vi.fn(async () => true);
  const request = commandRequest({
    reason: "Esta etapa usa um serviço pago. Estimativa do provedor: US$ 0,25.",
    command: { invocations: [{ argv: ["openmontage", "native-video-task"] }], redirections: [], dynamic: false },
    commandPrefixAvailable: false,
  });
  request.tool.name = "video_run";
  request.tool.args = {
    action: "tool",
    path: "videos/lancamento",
    tool: "video_generate",
    arguments: { provider: "fal", model: "kling-video", prompt: "Uma apresentação do produto." },
    costQuoteUsd: 0.25,
  };
  render(<ToolApproval request={request} projectPath="/projeto" onAnswer={answer} />);

  expect(screen.getByText("Autorizar etapa da produção de vídeo?")).toBeVisible();
  expect(screen.getByText("Produção")).toBeVisible();
  expect(screen.getByText("videos/lancamento")).toBeVisible();
  expect(screen.getByText("Ferramenta")).toBeVisible();
  expect(screen.getByText("video_generate")).toBeVisible();
  expect(screen.getByText("Parâmetros da ferramenta")).toBeVisible();
  expect(screen.getByText(/"provider": "fal"/)).toBeVisible();
  expect(screen.getByText(/"model": "kling-video"/)).toBeVisible();
  expect(screen.getByText("Estimativa informada (USD)")).toBeVisible();
  expect(screen.getByText("0.25")).toBeVisible();
  expect(screen.getByText(/Estimativa do provedor: US\$ 0,25/)).toBeVisible();
  expect(screen.queryByText("Comando interpretado")).not.toBeInTheDocument();
  expect(screen.queryByText("openmontage native-video-task")).not.toBeInTheDocument();
  expect(screen.queryByText("Autorizar alteração de arquivo?")).not.toBeInTheDocument();
  expect(answer).not.toHaveBeenCalled();

  await user.click(screen.getByRole("button", { name: "Autorizar uma vez" }));
  expect(answer).toHaveBeenCalledWith({ approved: true, grant: null });
});
