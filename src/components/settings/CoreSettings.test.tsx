import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { useCore } from "@/hooks/use-core";
import { coreFixture } from "@/test/core-fixtures";
import { CorePanel, CoreSettings } from "./CoreSettings";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
const invokeMock = vi.mocked(invoke);
const events = new Map<string, EventCallback<unknown>>();
const diagnosticSummary = {
  runId: "0123456789abcdef0123456789abcdef",
  appVersion: "0.9.10-beta",
  os: "macos",
  arch: "aarch64",
  startedAt: 1_757_376_000_000,
  logFiles: 1,
  logBytes: 1024,
  eventCount: 1,
  recentEvents: [],
  copyable: "Diagnóstico do Jarvis",
};
beforeEach(() => {
  invokeMock.mockReset(); events.clear();
  vi.mocked(toast.error).mockClear();
  vi.mocked(listen).mockImplementation(async (name, callback) => { events.set(name, callback); return () => { events.delete(name); }; });
});

it.each([false, true])("finishes update discovery despite status events and avoids rechecking on panel remount (failure: %s)", async fail => {
  const state = coreFixture();
  const message = "Não foi possível verificar atualizações do Core: O GitHub atingiu o limite temporário de consultas.";
  let finishCheck!: () => void;
  invokeMock.mockImplementation(async command => command === "check_core_updates"
    ? new Promise((resolve, reject) => { finishCheck = () => fail ? reject({ message }) : resolve(state); })
    : state);
  function Panel({ visible }: { visible: boolean }) {
    const core = useCore();
    return <>{core.checked && <p>Consulta concluída</p>}{visible && <CorePanel core={core} setup />}</>;
  }
  const view = render(<Panel visible />);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_core_updates"));
  await act(async () => {
    events.get("core:changed")?.({ event: "core:changed", id: 1, payload: { ...state, checking: true } });
    events.get("core:changed")?.({ event: "core:changed", id: 2, payload: state });
    finishCheck();
  });
  expect(await screen.findByText("Consulta concluída")).toBeVisible();
  expect(screen.getAllByText("Pronto")).toHaveLength(state.items.length);
  expect(screen.queryByRole("button", { name: /Reinstalar/ })).not.toBeInTheDocument();
  if (fail) expect(toast.error).toHaveBeenCalledWith(message);
  else expect(toast.error).not.toHaveBeenCalled();
  view.rerender(<Panel visible={false} />);
  view.rerender(<Panel visible />);
  expect(invokeMock.mock.calls.filter(([command]) => command === "check_core_updates")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "Verificar atualizações do Core" }));
  await waitFor(() => expect(invokeMock.mock.calls.filter(([command]) => command === "check_core_updates")).toHaveLength(2));
  await act(async () => finishCheck());
});

it.each(coreFixture().items.map((_, index) => index))("updates real download progress for Core item %i and resets it between stages", async index => {
  const state = coreFixture(false);
  state.items[index].stage = "Baixando recursos";
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  const bar = await screen.findByRole("progressbar", { name: `Instalação de ${state.items[index].name}` });
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_core_updates"));
  expect(bar).not.toHaveAttribute("aria-valuenow");
  expect(bar).toHaveAttribute("data-indeterminate");
  await act(async () => events.get("core:download")?.({ event: "core:download", id: 1, payload: { id: state.items[index].id, download: { receivedBytes: 1048576, totalBytes: 4194304 } } }));
  expect(bar).toHaveAttribute("aria-valuenow", "25");
  expect(screen.getByText("25%")).toBeInTheDocument();
  expect(screen.getByText("1 MB / 4 MB")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: `Instalar ${state.items[index].name}` })).toBeDisabled();
  state.items[index].stage = "Extraindo arquivos";
  await act(async () => events.get("core:changed")?.({ event: "core:changed", id: 2, payload: state }));
  expect(bar).not.toHaveAttribute("aria-valuenow");
  expect(screen.queryByText("25%")).not.toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("Extraindo arquivos");
  state.items[index].stage = null;
  state.items[index].error = "Download interrompido";
  await act(async () => events.get("core:changed")?.({ event: "core:changed", id: 3, payload: state }));
  expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  expect(screen.getByRole("alert")).toHaveTextContent("Download interrompido");
  expect(screen.getByRole("button", { name: `Instalar ${state.items[index].name}` })).toBeEnabled();
});

it("restores an ongoing download without inventing a percentage when the server omits its size", async () => {
  const state = coreFixture(false);
  state.items[3].stage = "Baixando recursos de design";
  state.items[3].download = { receivedBytes: 12582912, totalBytes: null };
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  const bar = await screen.findByRole("progressbar", { name: "Instalação de Impeccable" });
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_core_updates"));
  expect(bar).not.toHaveAttribute("aria-valuenow");
  expect(bar).toHaveAttribute("data-indeterminate");
  expect(screen.getByText("12 MB")).toBeInTheDocument();
  await act(async () => events.get("core:download")?.({ event: "core:download", id: 1, payload: { id: "impeccable", download: { receivedBytes: 16777216, totalBytes: null } } }));
  expect(screen.getByText("16 MB")).toBeInTheDocument();
});

it("preserves newer download progress when an earlier update check finishes", async () => {
  const state = coreFixture(false);
  state.items[3].stage = "Baixando recursos de design";
  state.items[3].download = { receivedBytes: 12582912, totalBytes: null };
  let finishCheck!: (value: unknown) => void;
  invokeMock.mockImplementation(async command => command === "check_core_updates"
    ? new Promise(resolve => { finishCheck = resolve; })
    : state);
  render(<CoreSettings />);
  await screen.findByText("12 MB");
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_core_updates"));
  await act(async () => events.get("core:download")?.({ event: "core:download", id: 1, payload: { id: "impeccable", download: { receivedBytes: 16777216, totalBytes: null } } }));
  expect(screen.getByText("16 MB")).toBeInTheDocument();
  await act(async () => finishCheck(state));
  expect(screen.getByText("16 MB")).toBeInTheDocument();
  expect(screen.queryByText("12 MB")).not.toBeInTheDocument();
});

it("explains the longer Impeccable installation through its help tooltip", async () => {
  const user = userEvent.setup(); invokeMock.mockResolvedValue(coreFixture());
  render(<CoreSettings />);
  await user.hover(await screen.findByRole("button", { name: "Sobre a instalação do Impeccable" }));
  expect(await screen.findByText(/O download e a preparação podem levar alguns minutos/)).toBeVisible();
});

it("offers Impeccable installation alongside the other Core resources", async () => {
  const state = coreFixture(); state.ready = false; state.items[3].installed = false; state.items[3].installedVersion = null;
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  fireEvent.click(await screen.findByRole("button", { name: "Instalar Impeccable" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "impeccable" }));
  expect(screen.getByText("7/8 essenciais")).toBeInTheDocument();
});

it("presents one OpenMontage production core for video, narration and music", async () => {
  invokeMock.mockResolvedValue(coreFixture());
  render(<CoreSettings />);
  expect(await screen.findByRole("heading", { name: "OpenMontage" })).toBeVisible();
  expect(screen.getByText(/Produção completa com OpenMontage/)).toHaveTextContent("narração, música, composição e revisão");
  expect(screen.queryByRole("heading", { name: "Hyperframes" })).not.toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Audiovisual" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Instalar Brag" })).not.toBeInTheDocument();
});

it.each(["impeccable", "openmontage", "comfyui", "graft"] as const)("offers required %s installation in the essential Core", async id => {
  const state = coreFixture();
  const video = state.items.find(item => item.id === id)!;
  video.installed = false; video.configured = false; video.installedVersion = null;
  state.ready = false;
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  fireEvent.click(await screen.findByRole("button", { name: `Instalar ${video.name}` }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id }));
  expect(screen.getByText("7/8 essenciais")).toBeVisible();
  expect(screen.queryByText("Opcional")).not.toBeInTheDocument();
});

it("distinguishes the production package from optional local resources and API integrations", async () => {
  invokeMock.mockResolvedValue(coreFixture(false));
  render(<CoreSettings />);
  expect(await screen.findByText(/Produção completa com OpenMontage/)).toHaveTextContent("conforme as dependências e credenciais configuradas");
});

it("shows openmontage model downloads through the existing progress events", async () => {
  const state = coreFixture();
  state.ready = false;
  const openmontage = state.items.find(item => item.id === "openmontage")!;
  openmontage.installed = false;
  openmontage.configured = false;
  openmontage.stage = "Baixando recursos do OpenMontage";
  openmontage.download = { receivedBytes: 1048576, totalBytes: null };
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  await screen.findByText("1 MB");
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_core_updates"));
  await act(async () => events.get("core:download")?.({ event: "core:download", id: 1, payload: { id: "openmontage", download: { receivedBytes: 2097152, totalBytes: null } } }));
  expect(screen.getByText("2 MB")).toBeVisible();
  expect(screen.getByText("Baixando recursos do OpenMontage")).toBeVisible();
});

it("cancels an openmontage download once, keeps progress until cleanup, and permits installation again", async () => {
  const state = coreFixture();
  const item = state.items.find(item => item.id === "openmontage")!;
  item.installed = false; item.configured = false; item.installedVersion = null;
  item.stage = "Baixando recursos do OpenMontage";
  item.download = { receivedBytes: 1048576, totalBytes: null };
  state.ready = false;
  let finishCancel!: (value: unknown) => void;
  invokeMock.mockImplementation(command => command === "cancel_core_installation" ? new Promise(resolve => { finishCancel = resolve; }) : Promise.resolve(state));
  render(<CoreSettings />);
  const cancel = await screen.findByRole("button", { name: "Cancelar instalação de OpenMontage" });
  fireEvent.click(cancel); fireEvent.click(cancel);
  expect(invokeMock.mock.calls.filter(([command]) => command === "cancel_core_installation")).toEqual([["cancel_core_installation", { id: "openmontage" }]]);
  expect(cancel).toBeDisabled();
  expect(cancel).toHaveTextContent("Cancelando…");
  await act(async () => finishCancel(state));
  expect(screen.getByRole("progressbar", { name: "Instalação de OpenMontage" })).toBeVisible();
  expect(cancel).toBeDisabled();
  const stopped = structuredClone(state);
  stopped.items.find(item => item.id === "openmontage")!.stage = null;
  await act(async () => events.get("core:changed")?.({ event: "core:changed", id: 1, payload: stopped }));
  expect(screen.queryByRole("button", { name: "Cancelar instalação de OpenMontage" })).not.toBeInTheDocument();
  expect(screen.queryByRole("progressbar", { name: "Instalação de OpenMontage" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Instalar OpenMontage" })).toBeEnabled();
});

it("does not resurrect download progress when cancellation returns after the cleanup event", async () => {
  const state = coreFixture();
  state.items.find(item => item.id === "openmontage")!.stage = "Baixando modelos";
  let finishCancel!: (value: unknown) => void;
  invokeMock.mockImplementation(command => command === "cancel_core_installation" ? new Promise(resolve => { finishCancel = resolve; }) : Promise.resolve(state));
  render(<CoreSettings />);
  fireEvent.click(await screen.findByRole("button", { name: "Cancelar instalação de OpenMontage" }));
  const stopped = structuredClone(state);
  stopped.items.find(item => item.id === "openmontage")!.stage = null;
  await act(async () => events.get("core:changed")?.({ event: "core:changed", id: 1, payload: stopped }));
  await act(async () => finishCancel(state));
  expect(screen.queryByRole("progressbar", { name: "Instalação de OpenMontage" })).not.toBeInTheDocument();
});

it("reports cancellation failures and leaves the cancel action available for retry", async () => {
  const state = coreFixture();
  state.items.find(item => item.id === "openmontage")!.stage = "Baixando modelos";
  invokeMock.mockImplementation(async command => {
    if (command === "cancel_core_installation") throw { message: "Não foi possível cancelar agora" };
    return state;
  });
  render(<CoreSettings />);
  fireEvent.click(await screen.findByRole("button", { name: "Cancelar instalação de OpenMontage" }));
  await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Não foi possível cancelar agora"));
  expect(screen.getByRole("button", { name: "Cancelar instalação de OpenMontage" })).toBeEnabled();
});

it("mostra versões e só oferece atualização quando há release maior", async () => {
  const state = coreFixture(); state.items[0].latestVersion = "1.1.0"; state.items[0].updateAvailable = true;
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  expect(await screen.findByRole("button", { name: "Atualizar Context-mode" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Atualizar Ponytail" })).not.toBeInTheDocument();
  expect(screen.getByText("→ 1.1.0")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Atualizar Context-mode" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "context-mode" }));
});

it.each(["Chave inválida", "Desbloqueie o cofre de credenciais do Linux e tente novamente."])("keeps Context7 configuration editable after %s", async message => {
  const state = coreFixture(); state.items[4].configured = false;
  let fail = true;
  invokeMock.mockImplementation(async command => {
    if (command === "configure_context7") { if (fail) throw { message }; return coreFixture(); }
    return state;
  });
  const user = userEvent.setup(); render(<CoreSettings />);
  await user.click(await screen.findByRole("button", { name: "Configurar Context7" }));
  expect(screen.getByText("A chave será validada e salva no cofre de credenciais do sistema.")).toBeVisible();
  const input = screen.getByLabelText("Chave de API");
  expect(input).toHaveAttribute("type", "password");
  expect(screen.getByRole("button", { name: "Salvar e verificar" })).toBeDisabled();
  await user.type(input, "test-only-key");
  await user.click(screen.getByRole("button", { name: "Salvar e verificar" }));
  expect(await screen.findByText(message)).toBeVisible();
  expect(input).toHaveValue("test-only-key");
  fail = false;
  await user.click(screen.getByRole("button", { name: "Salvar e verificar" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(invokeMock).toHaveBeenCalledWith("configure_context7", { apiKey: "test-only-key" });
});

it("mantém a versão instalada quando uma atualização falha e permite nova tentativa", async () => {
  const state = coreFixture(); state.items[0].updateAvailable = true; state.items[0].latestVersion = "1.1.0";
  invokeMock.mockImplementation(async command => { if (command === "install_core_component") { state.items[0].error = "Download interrompido"; throw { message: "Download interrompido" }; } return state; });
  render(<CoreSettings />);
  fireEvent.click(await screen.findByRole("button", { name: "Atualizar Context-mode" }));
  expect(await screen.findByText("Download interrompido")).toBeInTheDocument();
  expect(screen.getAllByText("v1.0.0")).toHaveLength(9);
  await waitFor(() => expect(screen.getByRole("button", { name: "Atualizar Context-mode" })).toBeEnabled());
});

it("oferece reinstalação no card e no diagnóstico após uma atualização falhar", async () => {
  const state = coreFixture();
  state.items[3].latestVersion = "1.1.0";
  state.items[3].updateAvailable = true;
  state.items[3].error = "Recursos da nova release inválidos";
  invokeMock.mockImplementation(async command => command === "get_diagnostic_summary" ? diagnosticSummary : state);
  render(<CoreSettings />);

  const cardReinstall = await screen.findByRole("button", { name: "Reinstalar Impeccable" });
  expect(screen.getByText("Atenção")).toBeInTheDocument();
  fireEvent.click(cardReinstall);
  expect(await screen.findByRole("alertdialog")).toHaveTextContent("baixada e verificada antes de substituir");
  fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));

  fireEvent.click(screen.getByRole("button", { name: "Diagnóstico e Reparo" }));
  const dialog = await screen.findByRole("dialog", { name: "Diagnóstico e Reparo" });
  expect(within(dialog).getByText("Core funcional · ação pendente")).toBeVisible();
  const diagnosticReinstall = within(dialog).getByRole("button", { name: "Reinstalar Impeccable" });
  await waitFor(() => expect(diagnosticReinstall).toBeEnabled());
  fireEvent.click(diagnosticReinstall);
  fireEvent.click(await screen.findByRole("button", { name: "Confirmar reinstalação" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("repair_core_component", { id: "impeccable", reinstall: true }));
});
