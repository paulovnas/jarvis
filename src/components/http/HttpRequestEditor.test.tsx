import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, expect, it, vi } from "vitest";
import { httpDraft } from "@/test/http-fixtures";
import type { HttpRequest } from "@/core/http-client";
import { HttpRequestEditor } from "./HttpRequestEditor";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); vi.mocked(open).mockReset(); });
function Editor({ initial = httpDraft().request, onChange }: { initial?: HttpRequest; onChange: (request: HttpRequest) => void }) {
  const [request, setRequest] = useState(initial);
  return <HttpRequestEditor projectId="project-http" request={request} onChange={next => { setRequest(next); onChange(next); }} disabled={false} />;
}

it("retains duplicate query keys and disabled rows while editing", async () => {
  const onChange = vi.fn(); const user = userEvent.setup();
  render(<Editor initial={{ ...httpDraft().request, params: [{ name: "tag", value: "a", enabled: true }, { name: "tag", value: "b", enabled: false }] }} onChange={onChange} />);
  expect(screen.getByRole("textbox", { name: "Nome parâmetro 1" })).toHaveValue("tag");
  expect(screen.getByRole("textbox", { name: "Nome parâmetro 2" })).toHaveValue("tag");
  await user.type(screen.getByRole("textbox", { name: "Valor parâmetro 1" }), "2");
  expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ params: [{ name: "tag", value: "a2", enabled: true }, { name: "tag", value: "b", enabled: false }] }));
  await user.click(screen.getByRole("checkbox", { name: "Ativar parâmetro 2" }));
  expect(onChange.mock.lastCall?.[0].params[1].enabled).toBe(true);
});

it("uses readable authentication labels and masks credentials", async () => {
  const onChange = vi.fn(); const user = userEvent.setup();
  render(<Editor initial={{ ...httpDraft().request, auth: { ...httpDraft().request.auth, type: "bearer", token: "{{token}}" } }} onChange={onChange} />);
  await user.click(screen.getByRole("tab", { name: "Autenticação" }));
  expect(screen.getByRole("combobox", { name: "Tipo de autenticação" })).toHaveTextContent("Bearer token");
  expect(screen.getByLabelText("Token ou variável")).toHaveAttribute("type", "password");
  expect(screen.getByLabelText("Token ou variável")).toHaveValue("{{token}}");
});

it.each(["binary", "multipart"] as const)("imports a %s upload through the native picker and stores only its file reference", async type => {
  const onChange = vi.fn(); const user = userEvent.setup();
  vi.mocked(open).mockResolvedValue("/tmp/image.png");
  vi.mocked(invoke).mockResolvedValue({ id: "file-1", name: "image.png", size: 128 });
  render(<Editor initial={{ ...httpDraft().request, body: { ...httpDraft().request.body, type, fields: [{ name: "photo", value: "", enabled: true, fileId: null }] } }} onChange={onChange} />);
  await user.click(screen.getByRole("tab", { name: "Corpo" }));
  await user.click(screen.getByRole("button", { name: type === "binary" ? "Selecionar arquivo" : "Arquivo multipart 1" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("import_http_file", { projectId: "project-http", path: "/tmp/image.png" }));
  await screen.findByText("image.png");
  const request = onChange.mock.lastCall?.[0] as HttpRequest;
  expect(type === "binary" ? request.body.fileId : request.body.fields[0].fileId).toBe("file-1");
  expect(JSON.stringify(request)).not.toContain("/tmp/image.png");
});
