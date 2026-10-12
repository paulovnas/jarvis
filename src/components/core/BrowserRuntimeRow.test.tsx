import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import type { BrowserRuntime } from "@/core/core-components";
import { BrowserRuntimeRow } from "./BrowserRuntimeRow";

const runtime: BrowserRuntime = {
  version: "151.8.1", state: "included",
  diagnostics: [{ label: "Motor Chromium", passed: true, message: "Incluído no aplicativo" }, { label: "Processos auxiliares do Chromium", passed: true, message: "Incluído no aplicativo" }],
};

it("identifies the bundled interactive browser without separate installation actions", () => {
  render(<BrowserRuntimeRow runtime={runtime} />);
  expect(screen.getByRole("heading", { name: "Navegador Chromium" })).toBeVisible();
  expect(screen.getByText("CEF 151.8.1")).toBeVisible();
  expect(screen.getByText("Incluído")).toBeVisible();
  expect(screen.getByText("Disponível")).toBeVisible();
  expect(screen.getByText("O Chromium acompanha o Jarvis e é atualizado com o aplicativo.")).toBeVisible();
  for (const action of ["Instalar", "Reparar", "Reinstalar", "Atualizar"]) expect(screen.queryByRole("button", { name: new RegExp(action) })).not.toBeInTheDocument();
  expect(screen.queryByRole("list", { name: "Verificações do Chromium" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Verificações do navegador Chromium" }));
  const checks = screen.getByRole("list", { name: "Verificações do Chromium" });
  expect(within(checks).getAllByRole("listitem")).toHaveLength(2);
  expect(checks).toHaveTextContent("Processos auxiliares do Chromium: Incluído no aplicativo");
});

it("directs damaged bundled browser recovery to the app and keeps the failure visible", () => {
  render(<BrowserRuntimeRow runtime={{ ...runtime, state: "unavailable", diagnostics: [{ label: "Motor Chromium", passed: false, message: "Motor Chromium ausente ou incompleto no aplicativo." }] }} />);
  expect(screen.getByRole("alert")).toHaveTextContent("Motor Chromium ausente ou incompleto no aplicativo.");
  expect(screen.getByText("Atualize ou reinstale o Jarvis para recuperar o navegador integrado.")).toBeVisible();
  expect(screen.getByText("Atenção")).toBeVisible();
  expect(screen.queryByRole("button", { name: /Instalar|Reparar|Reinstalar|Atualizar/ })).not.toBeInTheDocument();
});

it("explains development executables without falsely reporting runtime damage", () => {
  render(<BrowserRuntimeRow runtime={{ ...runtime, state: "development", diagnostics: [] }} />);
  expect(screen.getByText("Desenvolvimento")).toBeVisible();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Verificações do navegador Chromium" }));
  expect(screen.getByText("Executável de desenvolvimento; os arquivos incluídos são verificados no aplicativo empacotado.")).toBeVisible();
  expect(screen.getByText(/Os runtimes de renderização de vídeo são independentes/)).toBeVisible();
});
