import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { HyperPanelConnection } from "./HyperPanelConnection";

const { call, updateUsage, success, failure } = vi.hoisted(() => ({ call: vi.fn(), updateUsage: vi.fn(), success: vi.fn(), failure: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: call }));
vi.mock("@/hooks/use-bootstrap-resources", () => ({ useBootstrapResources: () => ({ updateUsage }) }));
vi.mock("sonner", () => ({ toast: { success, error: failure } }));

const alias = "charm-hyper-pessoal";
const disconnected = { connected: false, teamName: null, teamId: null, teams: [], flowId: null };
const waiting = { ...disconnected, flowId: "browser-flow" };
const connected = { ...disconnected, connected: true, teamName: "Minha equipe", teamId: "team-one" };
const props = () => ({ alias, disabled: false, onBusyChange: vi.fn() });

beforeEach(() => {
  call.mockReset().mockImplementation((command: string) => Promise.resolve(command === "start_charm_hyper_panel_connection" ? waiting : command === "finish_charm_hyper_panel_connection" ? connected : command === "disconnect_charm_hyper_panel" ? null : disconnected));
  updateUsage.mockReset(); success.mockReset(); failure.mockReset();
});

it("consulta o vínculo ao abrir sem iniciar login nem pedir credenciais", async () => {
  const options = props(); render(<HyperPanelConnection {...options} />);
  expect(await screen.findByRole("button", { name: "Conectar painel" })).toBeEnabled();
  expect(call).toHaveBeenCalledExactlyOnceWith("get_charm_hyper_panel_status", { alias });
  expect(screen.getByText("Requer a extensão do Jarvis e a aba do painel aberta no navegador. A sessão permanece no navegador.")).toBeVisible();
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(options.onBusyChange.mock.calls).toEqual([[true], [false]]);
});

it("bloqueia cliques duplicados e libera o formulário enquanto o login fica no navegador", async () => {
  let resolve!: (value: unknown) => void;
  call.mockImplementation((command: string) => command === "start_charm_hyper_panel_connection" ? new Promise(done => { resolve = done; }) : Promise.resolve(disconnected));
  const options = props(); render(<HyperPanelConnection {...options} />);
  const connect = await screen.findByRole("button", { name: "Conectar painel" });
  fireEvent.click(connect); fireEvent.click(connect);
  expect(call.mock.calls.filter(([command]) => command === "start_charm_hyper_panel_connection")).toHaveLength(1);
  expect(options.onBusyChange).toHaveBeenLastCalledWith(true);
  expect(screen.getByRole("button", { name: "Abrindo painel…" })).toBeDisabled();
  await act(async () => resolve(waiting));
  expect(screen.getByText("Conclua o login no Hyper no navegador e clique em Verificar conexão.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Verificar conexão" })).toBeEnabled();
  expect(options.onBusyChange).toHaveBeenLastCalledWith(false);
  expect(call.mock.calls.some(([command]) => command === "finish_charm_hyper_panel_connection")).toBe(false);
});

it("confirma a sessão e desconecta somente as cotas, atualizando o uso da conta", async () => {
  const focused = vi.fn(); window.addEventListener("focus", focused);
  try {
    const user = userEvent.setup(); render(<HyperPanelConnection {...props()} />);
    await user.click(await screen.findByRole("button", { name: "Conectar painel" }));
    await user.click(screen.getByRole("button", { name: "Verificar conexão" }));
    expect(call).toHaveBeenCalledWith("finish_charm_hyper_panel_connection", { alias, flowId: "browser-flow", teamId: null });
    expect(await screen.findByText(/Conectado à equipe Minha equipe/)).toBeVisible();
    expect(success).toHaveBeenCalledWith("Painel Hyper conectado");
    expect(updateUsage).toHaveBeenLastCalledWith(alias, { data: null, error: false });
    expect(focused).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Reconectar painel" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Desconectar cotas" }));
    expect(call).toHaveBeenCalledWith("disconnect_charm_hyper_panel", { alias });
    expect(await screen.findByRole("button", { name: "Conectar painel" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Desconectar cotas" })).not.toBeInTheDocument();
    expect(success).toHaveBeenLastCalledWith("Cotas do painel Hyper desconectadas");
    expect(updateUsage).toHaveBeenCalledTimes(2);
    expect(focused).toHaveBeenCalledTimes(2);
    expect(screen.getByText(/Desconectar cotas preserva a chave de API e o login no navegador/)).toBeVisible();
  } finally { window.removeEventListener("focus", focused); }
});

it("exige a escolha explícita da equipe quando a conta tem mais de uma", async () => {
  const teams = [{ id: "team-one", name: "Equipe A" }, { id: "team-two", name: "Equipe B" }];
  call.mockImplementation((command: string, args: Record<string, unknown>) => Promise.resolve(command === "get_charm_hyper_panel_status" ? disconnected : command === "start_charm_hyper_panel_connection" ? waiting : args.teamId === "team-two" ? { ...connected, teamId: "team-two", teamName: "Equipe B" } : { ...waiting, teams }));
  const user = userEvent.setup(); render(<HyperPanelConnection {...props()} />);
  await user.click(await screen.findByRole("button", { name: "Conectar painel" }));
  await user.click(screen.getByRole("button", { name: "Verificar conexão" }));
  expect(screen.getByRole("button", { name: "Verificar conexão" })).toBeDisabled();
  await user.click(screen.getByRole("combobox", { name: "Equipe do Hyper" }));
  await user.click(await screen.findByRole("option", { name: "Equipe B" }));
  await user.click(screen.getByRole("button", { name: "Verificar conexão" }));
  expect(call).toHaveBeenLastCalledWith("finish_charm_hyper_panel_connection", { alias, flowId: "browser-flow", teamId: "team-two" });
  expect(await screen.findByText(/Conectado à equipe Equipe B/)).toBeVisible();
});

it("exibe erros recuperáveis de conexão e permite tentar novamente por clique", async () => {
  let failed = true;
  call.mockImplementation((command: string) => command === "start_charm_hyper_panel_connection" && failed ? Promise.reject({ message: "Abra o navegador com a extensão do Jarvis." }) : Promise.resolve(command === "start_charm_hyper_panel_connection" ? waiting : disconnected));
  const user = userEvent.setup(); render(<HyperPanelConnection {...props()} />);
  await user.click(await screen.findByRole("button", { name: "Conectar painel" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Abra o navegador");
  expect(failure).toHaveBeenCalledWith("Abra o navegador com a extensão do Jarvis.");
  expect(screen.getByRole("button", { name: "Conectar painel" })).toBeEnabled();
  expect(updateUsage).not.toHaveBeenCalled();
  failed = false;
  await user.click(screen.getByRole("button", { name: "Conectar painel" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Verificar conexão" })).toBeEnabled();
});

it("mantém o vínculo quando a desconexão falha", async () => {
  call.mockImplementation((command: string) => command === "disconnect_charm_hyper_panel" ? Promise.reject(new Error("Não foi possível desconectar as cotas.")) : Promise.resolve(connected));
  const user = userEvent.setup(); render(<HyperPanelConnection {...props()} />);
  await user.click(await screen.findByRole("button", { name: "Desconectar cotas" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível desconectar");
  expect(screen.getByText(/Conectado à equipe Minha equipe/)).toBeVisible();
  expect(updateUsage).not.toHaveBeenCalled();
});

it("valida respostas IPC desconhecidas sem mostrar detalhes internos do schema", async () => {
  call.mockResolvedValue({ connected: "yes", teams: "not an array" });
  render(<HyperPanelConnection {...props()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível confirmar a conexão do painel Hyper.");
  expect(screen.getByRole("button", { name: "Conectar painel" })).toBeEnabled();
  expect(screen.getByRole("alert")).not.toHaveTextContent("Zod");
  expect(success).not.toHaveBeenCalled();
});

it("ignora o sucesso atrasado após fechar a tela e não altera o uso nem gera toast", async () => {
  let resolve!: (value: unknown) => void;
  call.mockImplementation((command: string) => command === "finish_charm_hyper_panel_connection" ? new Promise(done => { resolve = done; }) : Promise.resolve(command === "start_charm_hyper_panel_connection" ? waiting : disconnected));
  const options = props(); const user = userEvent.setup();
  const view = render(<HyperPanelConnection {...options} />);
  await user.click(await screen.findByRole("button", { name: "Conectar painel" }));
  await user.click(screen.getByRole("button", { name: "Verificar conexão" }));
  view.unmount();
  expect(options.onBusyChange).toHaveBeenLastCalledWith(false);
  const callsAfterUnmount = options.onBusyChange.mock.calls.length;
  await act(async () => resolve(connected));
  expect(success).not.toHaveBeenCalled(); expect(failure).not.toHaveBeenCalled();
  expect(updateUsage).not.toHaveBeenCalled();
  expect(options.onBusyChange).toHaveBeenCalledTimes(callsAfterUnmount);
});

it("ignora a consulta inicial atrasada depois de desmontar o componente", async () => {
  let resolve!: (value: unknown) => void;
  call.mockReturnValue(new Promise(done => { resolve = done; }));
  const options = props(); const view = render(<HyperPanelConnection {...options} />);
  expect(screen.getByRole("status", { name: "Consultando conexão do painel Hyper" })).toBeVisible();
  view.unmount();
  expect(options.onBusyChange.mock.calls).toEqual([[true], [false]]);
  await act(async () => resolve(connected));
  expect(options.onBusyChange.mock.calls).toEqual([[true], [false]]);
  expect(success).not.toHaveBeenCalled(); expect(failure).not.toHaveBeenCalled();
});

it("impede ações enquanto o formulário pai está salvando", async () => {
  const options = props(); const view = render(<HyperPanelConnection {...options} />);
  await screen.findByRole("button", { name: "Conectar painel" });
  view.rerender(<HyperPanelConnection {...options} disabled />);
  fireEvent.click(screen.getByRole("button", { name: "Conectar painel" }));
  await waitFor(() => expect(call).toHaveBeenCalledTimes(1));
});
