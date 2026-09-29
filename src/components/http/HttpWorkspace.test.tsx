import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { httpDraft, httpRun, httpSnapshot } from "@/test/http-fixtures";
import type { HttpRequest, HttpSnapshot } from "@/core/http-client";
import { useHttpClient } from "@/hooks/use-http-client";
import { FileWorkspace } from "@/components/files/FileWorkspace";
import { Button } from "@/components/ui/button";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
let stored: HttpSnapshot;
const analyze = vi.fn();
beforeEach(() => {
  stored = httpSnapshot(); analyze.mockReset();
  vi.mocked(invoke).mockReset().mockImplementation(async (command, args) => {
    if (command === "get_http_snapshot") return structuredClone(stored);
    if (command === "save_http_draft") {
      const input = args as { id: string | null; revision: number; request: HttpRequest; savedRequestId: string | null };
      const draft = httpDraft({ id: input.id ?? `draft-${stored.drafts.length + 1}`, revision: input.revision + 1, request: structuredClone(input.request), savedRequestId: input.savedRequestId });
      stored.drafts = [...stored.drafts.filter(item => item.id !== draft.id), draft]; return draft;
    }
    if (command === "close_http_draft") { stored.drafts = stored.drafts.filter(draft => draft.id !== (args as { id: string }).id); return; }
    if (command === "get_http_result") { const run = stored.runs.find(run => run.id === (args as { runId: string }).runId)!; return { run, text: run.preview, offset: 0, nextOffset: null, totalBytes: run.storedBytes, binary: false }; }
    if (command === "delete_http_request") { stored.savedRequests = []; stored.drafts = stored.drafts.map(draft => ({ ...draft, savedRequestId: null, revision: draft.revision + 1 })); return; }
    throw new Error(`Unexpected command ${command}`);
  });
});
function Workspace() {
  const http = useHttpClient("chat-http", "project-http");
  const [draft, setDraft] = useState("");
  return <FileWorkspace http={http} onAnalyzeHttp={analyze} terminalLauncher={<><Button onClick={() => void http.open()}>Nova requisição HTTP</Button><Button onClick={() => void http.refresh()}>Atualizar HTTP</Button></>}><input aria-label="Rascunho do chat" value={draft} onChange={event => setDraft(event.target.value)} /></FileWorkspace>;
}

it("keeps Chat mounted and preserves individual request edits while switching multiple tabs", async () => {
  const user = userEvent.setup(); render(<Workspace />);
  await user.type(screen.getByRole("textbox", { name: "Rascunho do chat" }), "Continuar conversa");
  await user.click(await screen.findByRole("tab", { name: /GETConsultar pedidos/ }));
  await user.clear(screen.getByRole("textbox", { name: "URL da requisição" }));
  await user.type(screen.getByRole("textbox", { name: "URL da requisição" }), "https://manual.test/orders");
  await user.click(screen.getByRole("button", { name: "Nova requisição HTTP" }));
  await screen.findByRole("tab", { name: /GETNova requisição/ });
  expect(screen.getByRole("textbox", { name: "URL da requisição" })).toHaveValue("");
  await user.click(screen.getByRole("tab", { name: /GETConsultar pedidos/ }));
  expect(screen.getByRole("textbox", { name: "URL da requisição" })).toHaveValue("https://manual.test/orders");
  await user.click(screen.getByRole("tab", { name: "Chat" }));
  expect(screen.getByRole("textbox", { name: "Rascunho do chat" })).toHaveValue("Continuar conversa");
  stored.runs = [httpRun()];
  await user.click(screen.getByRole("button", { name: "Atualizar HTTP" }));
  expect(screen.getByRole("tab", { name: "Chat" })).toHaveAttribute("aria-selected", "true");
});

it("requires an explicit close choice for an in-flight request and retains its history", async () => {
  stored.runs = [httpRun({ status: "running", httpStatus: null, finishedAt: null })];
  const user = userEvent.setup(); render(<Workspace />);
  await user.click(await screen.findByRole("button", { name: "Fechar requisição Consultar pedidos" }));
  expect(await screen.findByRole("dialog")).toHaveTextContent("Cancelar não desfaz efeitos");
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "close_http_draft")).toBe(false);
  await user.click(screen.getByRole("button", { name: "Manter execução e fechar" }));
  await waitFor(() => expect(stored.drafts).toHaveLength(0));
  expect(stored.runs[0].status).toBe("running");
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "cancel_http_request")).toBe(false);
});

it("analyzes an older selected run after background completion without replacing the edited request", async () => {
  stored.runs = [httpRun({ id: "newest", request: { ...httpDraft().request, name: "Nova execução" } }), httpRun({ id: "historical", request: { ...httpDraft().request, name: "Execução antiga" } })];
  const user = userEvent.setup(); render(<Workspace />);
  await user.click(await screen.findByRole("tab", { name: /GETConsultar pedidos/ }));
  await user.click(screen.getByRole("combobox", { name: "Execução HTTP selecionada" }));
  await user.click(await screen.findByRole("option", { name: /Execução antiga/ }));
  await screen.findByLabelText("Corpo da resposta");
  stored.runs.unshift(httpRun({ id: "background", request: { ...httpDraft().request, name: "Resultado em segundo plano" } }));
  await user.click(screen.getByRole("button", { name: "Atualizar HTTP" }));
  await user.click(screen.getByRole("button", { name: "Analisar com IA" }));
  expect(analyze).toHaveBeenCalledWith(expect.objectContaining({ id: "historical" }));
  expect(await screen.findByRole("textbox", { name: "Nome da requisição" })).toHaveValue("Consultar pedidos");
});

it("deletes a saved request only after confirmation while preserving its draft and result", async () => {
  stored.savedRequests = [{ id: "saved-1", projectId: "project-http", revision: 1, request: httpDraft().request }];
  stored.drafts[0].savedRequestId = "saved-1"; stored.runs = [httpRun()];
  const user = userEvent.setup(); render(<Workspace />);
  await user.click(await screen.findByRole("tab", { name: /GETConsultar pedidos/ }));
  await user.click(screen.getByRole("button", { name: "Excluir requisição salva" }));
  expect(screen.getByRole("alertdialog")).toHaveTextContent("histórico de execuções serão preservados");
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "delete_http_request")).toBe(false);
  await user.click(screen.getByRole("button", { name: "Excluir" }));
  await waitFor(() => expect(stored.savedRequests).toHaveLength(0));
  expect(stored.drafts).toHaveLength(1); expect(stored.runs).toHaveLength(1);
  expect(await screen.findByRole("textbox", { name: "Nome da requisição" })).toHaveValue("Consultar pedidos");
});
