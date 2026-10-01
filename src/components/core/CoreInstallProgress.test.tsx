import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { coreFixture } from "@/test/core-fixtures";
import { CoreInstallProgress } from "./CoreInstallProgress";

const item = { ...coreFixture().items.find(item => item.id === "audiovisual")!, stage: "Baixando modelos de voz e música", download: { receivedBytes: 1048576, totalBytes: null } };

it("offers cancellation for a running update while retaining real transfer progress", () => {
  const cancel = vi.fn();
  const view = render(<CoreInstallProgress item={item} operation="Atualização" onCancel={cancel} />);
  fireEvent.click(screen.getByRole("button", { name: "Cancelar atualização de Audiovisual" }));
  expect(cancel).toHaveBeenCalledOnce();
  expect(screen.getByText("1 MB")).toBeVisible();
  view.rerender(<CoreInstallProgress item={item} operation="Atualização" onCancel={cancel} cancelling />);
  const button = screen.getByRole("button", { name: "Cancelar atualização de Audiovisual" });
  expect(button).toBeDisabled();
  expect(button).toHaveTextContent("Cancelando…");
  fireEvent.click(button);
  expect(cancel).toHaveBeenCalledOnce();
});

it("does not offer installation cancellation for a readonly component diagnosis", () => {
  render(<CoreInstallProgress item={{ ...item, stage: "Analisando componente" }} onCancel={vi.fn()} />);
  expect(screen.getByRole("status")).toHaveTextContent("Analisando componente");
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});
