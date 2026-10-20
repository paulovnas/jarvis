import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { AssistantMessageTurn } from "./AssistantMessageTurn";
import type { ChatMessage, ToolCallItem } from "./types";
import { PixelArtResultsContext } from "@/hooks/use-pixel-art-results";
import { pixelArtResultSchema } from "@/core/pixel-art";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(vi.fn()) }));

const gif = { id: "walk", conversationId: "pixel-chat", name: "walk.gif", mime: "image/gif", kind: "image", size: 4096 };
const snapshot = {
  kind: "pixel_art", jobId: "generation-1", action: "generate", status: "running",
  phase: "Gerando quadros", progress: null, logs: ["Codex iniciado", "Imagem de referência carregada"],
  images: [], artifacts: [],
};
const tool = (id: string, data: object, status: ToolCallItem["status"] = "completed"): ToolCallItem => ({
  id, name: id === "run" ? "pixel_art_run" : "pixel_art_status", status, output: JSON.stringify(data),
});
const message = (tools: ToolCallItem[]): ChatMessage => ({
  id: "pixel-turn", role: "assistant", timestamp: "12:00", content: "",
  work: { durationSeconds: 12, steps: [{ thinking: "", commentary: "", tools }] },
});

it("shows real phases without inventing a percentage, then completed directions and original generation logs", async () => {
  const user = userEvent.setup();
  const { rerender } = render(<AssistantMessageTurn message={message([tool("run", snapshot)])} />);
  expect(screen.getByText("Gerando quadros")).toBeVisible();
  expect(screen.getByRole("progressbar", { name: "Progresso de pixel art" })).not.toHaveAttribute("aria-valuenow");
  expect(screen.queryByText(/96%/)).not.toBeInTheDocument();
  rerender(<AssistantMessageTurn message={message([tool("run", snapshot), tool("poll", {
    ...snapshot, phase: "Verificando direções", progress: { completed: 2, total: 8, unit: "direções" },
  })])} />);
  expect(screen.getAllByRole("region", { name: "Pixel art" })).toHaveLength(1);
  expect(screen.getByRole("progressbar", { name: "Progresso de pixel art" })).toHaveAttribute("aria-valuenow", "25");
  expect(screen.getByText("2 / 8 direções")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Logs da geração" }));
  expect(screen.getByText(/Codex iniciado\s+Imagem de referência carregada/)).toBeVisible();
});

it("keeps successful state reads and parameter changes in the work log instead of adding empty result cards", () => {
  render(<AssistantMessageTurn message={message([
    tool("configuration", { ...snapshot, jobId: "setup", action: "configure", status: "completed", phase: "Configuração atualizada" }),
    tool("inspection", { ...snapshot, jobId: "inspect", action: "inspect", status: "completed", phase: "Estado consultado" }),
    tool("run", snapshot),
  ])} />);
  expect(screen.getAllByRole("region", { name: "Pixel art" })).toHaveLength(1);
  expect(screen.getByText("Gerando quadros")).toBeVisible();
});

it("renders the animated GIF result once across polling and opens its original image", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockResolvedValue("data:image/gif;base64,R0lGODlh");
  const finished = { ...snapshot, status: "completed", phase: "Animação exportada", images: [gif], artifacts: [{ name: "walk.gif", path: "output/walk.gif", mime: "image/gif" }] };
  render(<AssistantMessageTurn message={message([tool("run", snapshot), tool("poll-1", finished), tool("poll-2", finished)])} />);
  expect(await screen.findAllByRole("img", { name: "walk.gif" })).toHaveLength(1);
  expect(screen.getByRole("img", { name: "walk.gif" })).toHaveAttribute("src", "data:image/gif;base64,R0lGODlh");
  expect(invoke).toHaveBeenCalledWith("get_chat_attachment_image", { conversationId: "pixel-chat", id: "walk", full: true });
  await user.click(screen.getByRole("button", { name: "Ampliar walk.gif" }));
  const dialog = await screen.findByRole("dialog");
  expect(await within(dialog).findByRole("img", { name: "walk.gif" })).toBeInTheDocument();
});

it("preserves partial images and explains a real blocker without displaying completion", async () => {
  vi.mocked(invoke).mockResolvedValue("data:image/gif;base64,R0lGODlh");
  const blocked = { ...snapshot, status: "blocked", phase: "Geração interrompida", images: [gif],
    error: "O Codex não disponibilizou image_gen nesta execução.",
    nextAction: "Reconecte a conta do Codex e retome este trabalho." };
  render(<AssistantMessageTurn message={message([tool("run", blocked, "error")])} />);
  const card = screen.getByRole("region", { name: "Pixel art" });
  expect(within(card).getByText("Bloqueado")).toBeVisible();
  expect(within(card).getByRole("alert")).toHaveTextContent(blocked.error);
  expect(within(card).getByText(blocked.nextAction)).toBeVisible();
  expect(await within(card).findByRole("img", { name: "walk.gif" })).toBeInTheDocument();
  expect(within(card).queryByText("Concluído")).not.toBeInTheDocument();
});

it("uses the saved final receipt after reopening a chat whose last tool output was still running", () => {
  const persisted = pixelArtResultSchema.parse({ ...snapshot, conversationId: "pixel-chat", revision: 4, status: "completed", phase: "Animação salva no projeto" });
  render(<PixelArtResultsContext value={{ [persisted.jobId]: persisted }}>
    <AssistantMessageTurn message={message([tool("run", { ...snapshot, conversationId: "pixel-chat", revision: 1 })])} />
  </PixelArtResultsContext>);
  expect(screen.getByText("Animação salva no projeto")).toBeVisible();
  expect(screen.queryByRole("progressbar", { name: "Progresso de pixel art" })).not.toBeInTheDocument();
  expect(within(screen.getByRole("region", { name: "Pixel art" })).getByText("Concluído")).toBeVisible();
});

it("shows saved GIF exports when history defers tool outputs without attaching another call's result", async () => {
  vi.mocked(invoke).mockResolvedValue("data:image/gif;base64,R0lGODlh");
  const exported = pixelArtResultSchema.parse({ ...snapshot, callId: "run", conversationId: "pixel-chat", revision: 4,
    action: "export", status: "completed", phase: "GIF exportado", images: [gif] });
  const unrelated = pixelArtResultSchema.parse({ ...exported, jobId: "another-job", callId: "another-call", phase: "Outro resultado" });
  render(<PixelArtResultsContext value={{ [exported.jobId]: exported, [unrelated.jobId]: unrelated }}>
    <AssistantMessageTurn message={message([{ id: "run", name: "pixel_art_run", status: "completed", output: "" }])} />
  </PixelArtResultsContext>);
  expect(await screen.findByRole("img", { name: "walk.gif" })).toBeVisible();
  expect(screen.getAllByRole("region", { name: "Pixel art" })).toHaveLength(1);
  expect(screen.queryByText("Outro resultado")).not.toBeInTheDocument();
});

it("requires an exact saved job when separate executors reuse the same provider call ID", async () => {
  vi.mocked(invoke).mockResolvedValue("data:image/gif;base64,R0lGODlh");
  const main = pixelArtResultSchema.parse({ ...snapshot, callId: "shared-call", conversationId: "pixel-chat",
    action: "export", status: "completed", phase: "Resultado principal", images: [gif] });
  const worker = pixelArtResultSchema.parse({ ...main, jobId: "worker-export", phase: "Resultado do especialista",
    images: [{ ...gif, id: "worker-image", name: "worker.gif" }] });
  const deferred: ToolCallItem = { id: "shared-call", name: "pixel_art_run", status: "completed", output: "" };
  const saved = { [main.jobId]: main, [worker.jobId]: worker };
  const view = render(<PixelArtResultsContext value={saved}><AssistantMessageTurn message={message([deferred])} /></PixelArtResultsContext>);
  expect(screen.queryByRole("region", { name: "Pixel art" })).not.toBeInTheDocument();
  view.rerender(<PixelArtResultsContext value={saved}><AssistantMessageTurn message={message([
    { ...deferred, id: "later-status-query", name: "pixel_art_status", args: { _jarvisHistoryDetailsDeferred: true, __pixelArtJobId: worker.jobId } },
  ])} /></PixelArtResultsContext>);
  expect(await screen.findByRole("img", { name: "worker.gif" })).toBeVisible();
  expect(screen.queryByRole("img", { name: "walk.gif" })).not.toBeInTheDocument();
});

it("explains a preview limit while keeping the successfully exported original available", () => {
  render(<AssistantMessageTurn message={message([tool("export", {
    ...snapshot, action: "export", status: "completed", phase: "Arquivo exportado",
    artifacts: [{ name: "large.gif", path: "C:/project/large.gif", mime: "image/gif", previewError: "O arquivo foi salvo, mas excede o limite de prévia do chat." }],
  })])} />);
  expect(screen.getByText("O arquivo foi salvo, mas excede o limite de prévia do chat.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Copiar caminho de large.gif" })).toBeVisible();
  expect(within(screen.getByRole("region", { name: "Pixel art" })).getByText("Concluído")).toBeVisible();
});

it("saves a complete export package using the native artifact dialog", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockResolvedValue(true);
  render(<AssistantMessageTurn message={message([tool("export", {
    ...snapshot, conversationId: "pixel-chat", action: "export", status: "completed", phase: "Pacote pronto",
    artifacts: [{ name: "walk.zip", path: "C:/session/walk.zip", mime: "application/zip" }],
  })])} />);
  await user.click(screen.getByRole("button", { name: "Salvar walk.zip" }));
  expect(invoke).toHaveBeenCalledWith("save_pixel_art_artifact", { conversationId: "pixel-chat", jobId: snapshot.jobId, path: "C:/session/walk.zip" });
});

it("updates the real native progress without accepting another chat's events or regressing to stale snapshots", async () => {
  let notify: EventCallback<unknown> | undefined;
  vi.mocked(listen).mockImplementationOnce(async (_event, callback) => { notify = callback; return vi.fn<() => void>(); });
  const initial = { ...snapshot, conversationId: "pixel-chat", revision: 1 };
  const view = render(<AssistantMessageTurn message={message([tool("run", initial)])} />);
  const update = (data: object) => act(() => notify?.({ event: "pixel-art-progress", id: 1, payload: data }));
  update({ ...initial, conversationId: "another-chat", phase: "Não mostrar", revision: 9 });
  expect(screen.queryByText("Não mostrar")).not.toBeInTheDocument();
  update({ ...initial, phase: "Verificando saída", revision: 3 });
  expect(screen.getByText("Verificando saída")).toBeVisible();
  update({ ...initial, phase: "Estado antigo", revision: 2 });
  expect(screen.queryByText("Estado antigo")).not.toBeInTheDocument();
  view.rerender(<AssistantMessageTurn message={message([tool("run", { ...initial, status: "completed", phase: "Arquivos preservados", revision: 4 })])} />);
  expect(screen.getByText("Arquivos preservados")).toBeVisible();
});
