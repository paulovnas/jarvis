import { useState } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { markdownSource } from "@/test/markdown-editor";
import { MarkdownEditor } from "./MarkdownEditor";

function Harness({ initial = "", maxLength }: { initial?: string; maxLength?: number }) {
  const [value, setValue] = useState(initial);
  return <MarkdownEditor label="Documento" value={value} onChange={setValue} maxLength={maxLength} />;
}

it("renders Markdown and preserves the exact source when switching modes without edits", async () => {
  const value = "# Documento\n\nTexto **importante** e [link](https://example.com).\n\n- Item\n\n- [x] Feito\n\n| Coluna | Valor |\n| --- | --- |\n| API | REST |\n\n```ts\nconst valor = 1;\n```\n";
  const change = vi.fn();
  const user = userEvent.setup();
  render(<MarkdownEditor label="Documento" value={value} onChange={change} />);
  expect(screen.getByRole("heading", { name: "Documento" })).toBeVisible();
  expect(screen.getByText("importante").tagName).toBe("STRONG");
  expect(screen.getByRole("table")).toHaveTextContent("API");
  expect(screen.getByRole("checkbox")).toBeChecked();
  expect(await markdownSource(user, "Documento")).toHaveValue(value);
  await user.click(screen.getByRole("tab", { name: "Editor" }));
  expect(screen.getByRole("heading", { name: "Documento" })).toBeVisible();
  expect(change).not.toHaveBeenCalled();
});

it("edits visually, supports formatting and undo, and reads edits made in code mode", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click(screen.getByRole("button", { name: "Negrito" }));
  await user.type(screen.getByRole("textbox", { name: "Documento" }), "Importante");
  expect(await markdownSource(user, "Documento")).toHaveValue("**Importante**");
  await user.click(screen.getByRole("tab", { name: "Editor" }));
  await user.click(screen.getByRole("button", { name: "Desfazer" }));
  expect(screen.getByRole("textbox", { name: "Documento" })).not.toHaveTextContent("Importante");
  const source = await markdownSource(user, "Documento");
  fireEvent.change(source, { target: { value: "## Revisado\n\n- Primeiro\n- Segundo" } });
  await user.click(screen.getByRole("tab", { name: "Editor" }));
  expect(screen.getByRole("heading", { name: "Revisado", level: 2 })).toBeVisible();
  expect(screen.getAllByRole("listitem")).toHaveLength(2);
});

it("updates externally loaded content and respects read-only, disabled and length limits", async () => {
  const user = userEvent.setup();
  const view = render(<Harness maxLength={5} />);
  await user.type(screen.getByRole("textbox", { name: "Documento" }), "123456");
  expect(await markdownSource(user, "Documento")).toHaveValue("12345");
  const change = vi.fn();
  view.rerender(<MarkdownEditor label="Documento" value="# Externo" onChange={change} readOnly />);
  expect(screen.getByRole("heading", { name: "Externo" })).toBeVisible();
  expect(screen.getByRole("textbox", { name: "Documento" })).toHaveAttribute("contenteditable", "false");
  expect(await markdownSource(user, "Documento")).toHaveAttribute("readonly");
  view.rerender(<MarkdownEditor label="Documento" value="# Atualizado" onChange={change} disabled />);
  expect(screen.getByRole("textbox", { name: "Documento" })).toBeDisabled();
  expect(change).not.toHaveBeenCalled();
});

it("pastes formatted Markdown, preserves structured content after editing, and keeps code paste literal", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click(screen.getByRole("textbox", { name: "Documento" }));
  await user.paste("# Documento\n\n[Referência](https://example.com)\n\n| Área | Tipo |\n| --- | --- |\n| API | REST |\n\n- [ ] Revisar\n\n```sh\necho ok\n```");
  expect(screen.getByRole("heading", { name: "Documento" })).toBeVisible();
  await user.click(screen.getByRole("checkbox", { name: "Concluir: Revisar" }));
  await user.click(screen.getByText("echo ok"));
  await user.paste("# literal");
  const source = await markdownSource(user, "Documento");
  expect((source as HTMLTextAreaElement).value).toContain("[x] Revisar");
  expect((source as HTMLTextAreaElement).value).toContain("[Referência](https://example.com)");
  await user.click(screen.getByRole("tab", { name: "Editor" }));
  expect(screen.getByRole("table")).toHaveTextContent("REST");
  expect(screen.getByRole("checkbox", { name: "Concluir: Revisar" })).toBeChecked();
  const code = within(screen.getByRole("textbox", { name: "Documento" })).getByText(/echo ok/);
  expect(code.tagName).toBe("CODE");
  expect(code).toHaveTextContent("# literal");
});

it("preserves metadata and embedded HTML through source editing instead of silently discarding them", async () => {
  const value = "---\ntitle: Produto\n---\n\n# Produto\n\n<!-- requisito -->\n<div data-important='true'>Detalhe</div>";
  const user = userEvent.setup();
  render(<Harness initial={value} />);
  expect(screen.getByRole("alert")).toHaveTextContent("Use Código");
  expect(screen.getByRole("textbox", { name: "Documento" })).toHaveAttribute("contenteditable", "false");
  const source = await markdownSource(user, "Documento");
  expect(source).toHaveValue(value);
  fireEvent.change(source, { target: { value: `${value}\n\nNovo requisito` } });
  await user.click(screen.getByRole("tab", { name: "Editor" }));
  expect(within(screen.getByRole("textbox", { name: "Documento" })).getByText("Novo requisito")).toBeVisible();
  expect(await markdownSource(user, "Documento")).toHaveValue(`${value}\n\nNovo requisito`);
});
