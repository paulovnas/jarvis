import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { initialBootstrapProgress } from "@/core/bootstrap-state";
import { BootstrapScreen } from "./BootstrapScreen";

describe("BootstrapScreen", () => {
  it("shows structural progress and the active startup step", () => {
    const steps = initialBootstrapProgress();
    steps.configuration = { id: "configuration", progress: 1, status: "complete", detail: "Configuração carregada" };
    steps.core = { id: "core", progress: 0.5, status: "running", detail: "Validando instalações e versões" };

    render(<BootstrapScreen steps={steps} />);

    expect(screen.getByRole("heading", { name: "Preparando seu ambiente" })).toBeVisible();
    expect(screen.getByRole("status", { name: "Iniciando o Jarvis" })).toHaveTextContent("24%");
    expect(screen.getAllByText("Validando instalações e versões")).toHaveLength(2);
    expect(screen.getByRole("progressbar", { name: "Progresso da inicialização" })).toHaveAttribute("aria-valuenow", "24");
  });

  it("keeps recoverable failures visible without replacing the bootstrap", () => {
    const steps = initialBootstrapProgress();
    steps.skills = { id: "skills", progress: 1, status: "warning", detail: "Skills carregadas; atualização indisponível" };

    render(<BootstrapScreen steps={steps} />);

    expect(screen.getByText("Skills carregadas; atualização indisponível")).toBeVisible();
    expect(screen.getByText("Skills instaladas").closest("[data-status]"))?.toHaveAttribute("data-status", "warning");
  });
});
