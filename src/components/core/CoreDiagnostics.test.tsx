import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { CORE_DETAILS } from "@/core/core-presentation";
import type { CoreController } from "@/hooks/use-core";
import { coreFixture } from "@/test/core-fixtures";
import CoreDiagnostics from "./CoreDiagnostics";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@/components/core/DiagnosticEvidence", () => ({ DiagnosticEvidence: () => <p>Evidências locais</p> }));

function controller(complete = true): CoreController {
  const snapshot = coreFixture();
  if (complete) {
    for (const item of snapshot.items) {
      item.diagnostics = [{ label: `Runtime de ${item.name}`, passed: true, message: "Disponível" }];
    }
  }
  return {
    snapshot, busy: false, checked: false, error: null, cancellingId: null,
    refresh: vi.fn(async () => {}), check: vi.fn(async () => {}),
    diagnose: vi.fn(async () => {}), repair: vi.fn(async () => {}),
    cancel: vi.fn(async () => {}), install: vi.fn(async () => ({ updated: 0, errors: [] })),
  };
}

it("shows bundled Chromium checks separately from package repair actions", () => {
  const core = controller();
  core.snapshot!.browserRuntime = { version: "151.8.1", state: "unavailable", diagnostics: [{ label: "Processos auxiliares do Chromium", passed: false, message: "Processos auxiliares ausentes no aplicativo." }] };
  render(<CoreDiagnostics core={core} onClose={vi.fn()} />);
  expect(screen.getByRole("heading", { name: "Navegador Chromium" })).toBeVisible();
  expect(screen.getByText("Core funcional · ação pendente")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("Processos auxiliares ausentes no aplicativo.");
  expect(screen.getByText("Atualize ou reinstale o Jarvis para recuperar o navegador integrado.")).toBeVisible();
  expect(screen.queryByRole("button", { name: /(?:Instalar|Reparar|Reinstalar).*Chromium/ })).not.toBeInTheDocument();
  expect(core.repair).not.toHaveBeenCalled();
  expect(core.install).not.toHaveBeenCalled();
});

it("directs bundled component recovery to the app instead of installing or repairing an external Core", () => {
  const core = controller(); const item = core.snapshot!.items[3];
  item.bundled = true; item.healthError = "Arquivo nativo ausente"; core.snapshot!.ready = false;
  render(<CoreDiagnostics core={core} onClose={vi.fn()} />);
  expect(screen.getByText("Incluído no Jarvis; atualizado com o aplicativo.")).toBeVisible();
  expect(screen.getByText("Atualize ou reinstale o Jarvis para recuperar este componente nativo.")).toBeVisible();
  for (const action of ["Instalar", "Reparar", "Reinstalar"]) expect(screen.queryByRole("button", { name: `${action} Impeccable` })).not.toBeInTheDocument();
});

it("describes every component and reuses completed diagnostics until explicitly refreshed", async () => {
  const core = controller();
  render(<CoreDiagnostics core={core} onClose={vi.fn()} />);
  for (const item of core.snapshot!.items) {
    expect(screen.getByText(CORE_DETAILS[item.id].summary)).toBeVisible();
    expect(screen.queryByText(CORE_DETAILS[item.id].description)).not.toBeInTheDocument();
    expect(screen.queryByText(`Runtime de ${item.name}`)).not.toBeInTheDocument();
  }
  expect(core.diagnose).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Analisar novamente" }));
  expect(core.diagnose).toHaveBeenCalledOnce();
});

it("mounts passed checks only when their details are expanded with the keyboard", async () => {
  const user = userEvent.setup();
  const core = controller();
  render(<CoreDiagnostics core={core} onClose={vi.fn()} />);
  const details = screen.getByRole("button", { name: "Verificações de Context-mode" });
  expect(details).toHaveAttribute("aria-expanded", "false");
  await user.tab();
  details.focus();
  expect(details).toHaveFocus();
  await user.keyboard("{Enter}");
  expect(details).toHaveAttribute("aria-expanded", "true");
  expect(screen.getByText("Runtime de Context-mode")).toBeVisible();
  expect(screen.getByText(CORE_DETAILS["context-mode"].description)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Verificações de Ponytail" }));
  expect(screen.queryByText("Runtime de Context-mode")).not.toBeInTheDocument();
  expect(screen.getByText("Runtime de Ponytail")).toBeVisible();
});

it("automatically analyzes missing diagnostics once and avoids duplicate work on rerenders", async () => {
  const core = controller(false);
  const view = render(<CoreDiagnostics core={core} onClose={vi.fn()} />);
  await waitFor(() => expect(core.diagnose).toHaveBeenCalledOnce());
  view.rerender(<CoreDiagnostics core={{ ...core, snapshot: structuredClone(core.snapshot) }} onClose={vi.fn()} />);
  expect(core.diagnose).toHaveBeenCalledOnce();
});

it("waits for a snapshot and an idle Core before analyzing missing checks", async () => {
  const core = controller(false);
  const view = render(<CoreDiagnostics core={{ ...core, snapshot: null }} onClose={vi.fn()} />);
  expect(screen.getByRole("status", { name: "Carregando diagnóstico" })).toBeVisible();
  expect(core.diagnose).not.toHaveBeenCalled();
  view.rerender(<CoreDiagnostics core={{ ...core, busy: true }} onClose={vi.fn()} />);
  expect(core.diagnose).not.toHaveBeenCalled();
  view.rerender(<CoreDiagnostics core={core} onClose={vi.fn()} />);
  await waitFor(() => expect(core.diagnose).toHaveBeenCalledOnce());
});

it("keeps failed checks and their recovery action visible without expanding passed checks", async () => {
  const core = controller();
  const item = core.snapshot!.items[0];
  item.healthError = "Runtime indisponível";
  item.diagnostics.push({ label: "Servidor MCP", passed: false, message: "Não respondeu. Tente reparar o componente." });
  core.snapshot!.ready = false;
  render(<CoreDiagnostics core={core} onClose={vi.fn()} />);
  expect(screen.getByText("Servidor MCP")).toBeVisible();
  expect(screen.getByText("Não respondeu. Tente reparar o componente.")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("Runtime indisponível");
  expect(screen.queryByText("Runtime de Context-mode")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Reparar Context-mode" }));
  expect(core.repair).toHaveBeenCalledWith("context-mode", false);
});

it("preserves installation and OpenMontage configuration actions", async () => {
  const core = controller();
  const item = core.snapshot!.items[0];
  item.installed = false; item.installedVersion = null; item.configured = false;
  core.snapshot!.ready = false;
  render(<CoreDiagnostics core={core} onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Instalar Context-mode" }));
  expect(core.install).toHaveBeenCalledWith(["context-mode"]);
  expect(screen.queryByRole("button", { name: "Configurar OpenMontage" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Verificações de OpenMontage" }));
  expect(screen.getByRole("button", { name: "Configurar OpenMontage" })).toBeVisible();
});

it("allows closing during diagnosis while preventing duplicate analysis and installation cancellation", () => {
  const core = controller(false);
  core.busy = true;
  core.snapshot!.items[0].stage = "Analisando componente";
  const onClose = vi.fn();
  render(<CoreDiagnostics core={core} onClose={onClose} />);
  expect(screen.getByRole("progressbar", { name: "Instalação de Context-mode" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Analisar novamente" })).toBeDisabled();
  expect(screen.queryByRole("button", { name: "Cancelar instalação de Context-mode" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Voltar ao Jarvis" }));
  expect(onClose).toHaveBeenCalledOnce();
  expect(core.cancel).not.toHaveBeenCalled();
});

it("keeps installation progress cancellable and blocks dismissal while installing", () => {
  const core = controller();
  core.busy = true;
  core.snapshot!.items[0].stage = "Baixando recursos";
  const onClose = vi.fn();
  render(<CoreDiagnostics core={core} onClose={onClose} />);
  expect(screen.getByRole("button", { name: "Voltar ao Jarvis" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Cancelar instalação de Context-mode" }));
  expect(core.cancel).toHaveBeenCalledWith("context-mode");
  expect(onClose).not.toHaveBeenCalled();
});
