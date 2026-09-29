import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { beforeEach, expect, it, vi } from "vitest";
import { httpRun } from "@/test/http-fixtures";
import { HttpResponse } from "./HttpResponse";
import { writeClipboardText } from "@/core/clipboard";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("@/core/clipboard", () => ({ writeClipboardText: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); vi.mocked(save).mockReset(); vi.mocked(writeClipboardText).mockReset(); });

it("treats HTTP 500 as an inspectable result and analyzes the exact stored run without resending", async () => {
  const run = httpRun({ id: "historical-500", httpStatus: 500 });
  vi.mocked(invoke).mockResolvedValue({ run, text: '{"error":"Unavailable"}', offset: 0, nextOffset: null, totalBytes: 23, binary: false });
  const analyze = vi.fn(); const user = userEvent.setup();
  render(<HttpResponse conversationId="chat-http" run={run} onAnalyze={analyze} />);
  expect(await screen.findByLabelText("Corpo da resposta")).toHaveTextContent('"error": "Unavailable"');
  expect(screen.getByText("500")).toBeVisible();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Analisar com IA" }));
  expect(analyze).toHaveBeenCalledWith(run);
  expect(invoke).toHaveBeenCalledWith("get_http_result", { conversationId: "chat-http", runId: "historical-500", offset: 0, limit: 65536 });
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "send_http_request")).toBe(false);
  await user.click(screen.getByRole("button", { name: "Copiar trecho" }));
  expect(writeClipboardText).toHaveBeenCalledWith('{\n  "error": "Unavailable"\n}');
});

it("renders HTML response as inert text and distinguishes an uncertain transport outcome", async () => {
  const run = httpRun({ status: "failed", httpStatus: null, outcomeUncertain: true, error: "Conexão encerrada", mime: "text/html" });
  const html = '<script>window.injected = true</script><button>Enviar pagamento</button>';
  vi.mocked(invoke).mockResolvedValue({ run, text: html, offset: 0, nextOffset: null, totalBytes: html.length, binary: false });
  render(<HttpResponse conversationId="chat-http" run={run} onAnalyze={vi.fn()} />);
  expect(await screen.findByLabelText("Corpo da resposta")).toHaveTextContent(html);
  expect(screen.queryByRole("button", { name: "Enviar pagamento" })).not.toBeInTheDocument();
  expect(screen.getByText(/O servidor pode ter aplicado a operação/)).toBeVisible();
  expect(screen.getByText("Conexão encerrada")).toBeVisible();
});

it("offers lossless native saving for binary data without showing bytes as text", async () => {
  const run = httpRun({ mime: "application/octet-stream", preview: "" });
  vi.mocked(invoke).mockResolvedValue({ run, text: "", offset: 0, nextOffset: null, totalBytes: 12, binary: true });
  vi.mocked(save).mockResolvedValue("/tmp/response.bin");
  const user = userEvent.setup(); render(<HttpResponse conversationId="chat-http" run={run} onAnalyze={vi.fn()} />);
  expect(await screen.findByText("Resposta binária")).toBeVisible();
  expect(screen.getByRole("button", { name: "Copiar trecho" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Salvar original" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_http_response", { conversationId: "chat-http", runId: "run-1", path: "/tmp/response.bin" }));
});

it("shows empty 204 responses and prevents export of an expired body", async () => {
  const run = httpRun({ httpStatus: 204, preview: "", storedBytes: 0, receivedBytes: 0 });
  vi.mocked(invoke).mockResolvedValue({ run, text: "", offset: 0, nextOffset: null, totalBytes: 0, binary: false });
  const { rerender } = render(<HttpResponse conversationId="chat-http" run={run} onAnalyze={vi.fn()} />);
  expect(await screen.findByLabelText("Corpo da resposta")).toHaveTextContent("Resposta sem corpo.");
  expect(screen.getByText("204")).toBeVisible();
  rerender(<HttpResponse conversationId="chat-http" run={{ ...run, bodyExpired: true }} onAnalyze={vi.fn()} />);
  expect(screen.getByText(/O corpo desta resposta expirou/)).toBeVisible();
  expect(screen.getByRole("button", { name: "Salvar original" })).toBeDisabled();
});
