import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { AssistantMessageTurn } from "./AssistantMessageTurn";
import type { ChatMessage, ToolCallItem } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const image = { id: "a1", conversationId: "chat1", name: "imagem-1.png", mime: "image/png", kind: "image", size: 1200 };
const result = JSON.stringify({ kind: "generated_image", model: "gemini-3.1-flash-image", accountAlias: "google", images: [image], text: "" });
const message = (tool: ToolCallItem): ChatMessage => ({ id: "m1", role: "assistant", timestamp: "12:00", content: "", streaming: tool.status === "running", work: { durationSeconds: 1, steps: [{ thinking: "", commentary: "", tools: [tool] }] } });

it("shows a skeleton without opening activity, then the persisted image and export dialog", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockImplementation(async command => command === "save_chat_image" ? true : "data:image/png;base64,aW1hZ2U=");
  const tool: ToolCallItem = { id: "t1", name: "generate_image", status: "running" };
  const { rerender } = render(<AssistantMessageTurn message={message(tool)} />);
  expect(screen.getByRole("status", { name: "Gerando imagem" })).toBeVisible();
  rerender(<AssistantMessageTurn message={message({ ...tool, status: "completed", output: result })} />);
  expect(screen.queryByRole("status", { name: "Gerando imagem" })).not.toBeInTheDocument();
  expect(screen.getByRole("status", { name: "Carregando imagem" })).toBeVisible();
  const img = await screen.findByRole("img", { name: image.name });
  fireEvent.load(img);
  await user.click(screen.getByRole("button", { name: `Ampliar ${image.name}` }));
  const dialog = await screen.findByRole("dialog");
  expect(dialog).toBeVisible();
  expect(within(dialog).getByText("1,2 KB")).toBeVisible();
  const fullImage = await within(dialog).findByRole("img", { name: image.name });
  Object.defineProperties(fullImage, { naturalWidth: { value: 1536 }, naturalHeight: { value: 1024 } });
  fireEvent.load(fullImage);
  expect(within(dialog).getByText("1536 × 1024 px")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Salvar imagem" }).parentElement).toHaveClass("justify-center");
  await user.click(screen.getByRole("button", { name: "Salvar imagem" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_chat_image", { conversationId: "chat1", id: "a1" }));
});
it("uses the logo as the J of the wordmark and announces the full brand name", () => {
  render(<AssistantMessageTurn message={message({ id: "t1", name: "read_file", status: "completed" })} />);
  const brand = screen.getByRole("img", { name: "Jarvis" });
  expect(within(brand).getByText("arvis")).toBeVisible();
  expect(screen.queryByText("Jarvis")).not.toBeInTheDocument();
});
it("replaces the skeleton with the failed generation message", () => {
  render(<AssistantMessageTurn message={message({ id: "t1", name: "generate_image", status: "error", output: "O limite de geração de imagens foi atingido." })} />);
  expect(screen.getByRole("alert")).toHaveTextContent("O limite de geração");
  expect(screen.queryByRole("status", { name: "Gerando imagem" })).not.toBeInTheDocument();
});

it("shows the delegated specialist result once and identifies the actual image model", async () => {
  vi.mocked(invoke).mockResolvedValue("data:image/png;base64,aW1hZ2U=");
  const output = JSON.stringify({ kind: "generated_image", accountAlias: "configured-images", model: "configured-image-model", images: [image], text: "", sourcePaths: ["output/images/banner.webp"], agentId: "image-worker", role: "image_generator", processing: { engine: "comfyui" } });
  render(<AssistantMessageTurn message={message({ id: "image-facade", name: "generate_image", status: "completed", output })} />);
  expect(await screen.findAllByRole("img", { name: image.name })).toHaveLength(1);
  expect(screen.getAllByRole("button", { name: `Ampliar ${image.name}` })).toHaveLength(1);
  expect(screen.getByText("configured-image-model")).toHaveAttribute("title", "configured-images / configured-image-model");
  expect(screen.queryByText("Gemini 3.1 Flash Image")).not.toBeInTheDocument();
});

it("renders a processed image returned by a direct specialist edit", async () => {
  vi.mocked(invoke).mockResolvedValue("data:image/png;base64,aW1hZ2U=");
  render(<AssistantMessageTurn message={message({ id: "processed-image", name: "image_process", status: "completed", output: result })} />);
  expect(await screen.findByRole("img", { name: image.name })).toBeVisible();
  expect(screen.getByRole("button", { name: `Ampliar ${image.name}` })).toBeVisible();
});

it("shows the verified source resolution while keeping the memory-bounded preview", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockResolvedValue("data:image/png;base64,aW1hZ2U=");
  const output = JSON.stringify({ kind: "generated_image", accountAlias: "images", model: "image-model", images: [image], text: "", processing: { engine: "comfyui", images: [{ path: "image-01.png", width: 4096, height: 3072 }] } });
  render(<AssistantMessageTurn message={message({ id: "large-output", name: "generate_image", status: "completed", output })} />);
  await user.click(screen.getByRole("button", { name: `Ampliar ${image.name}` }));
  const dialog = await screen.findByRole("dialog");
  const preview = await within(dialog).findByRole("img", { name: image.name });
  Object.defineProperties(preview, { naturalWidth: { value: 2048 }, naturalHeight: { value: 1536 } });
  fireEvent.load(preview);
  expect(within(dialog).getByText("4096 × 3072 px")).toBeVisible();
  expect(within(dialog).queryByText("2048 × 1536 px")).not.toBeInTheDocument();
  expect(within(dialog).getByText("1,2 KB")).toBeVisible();
  expect(invoke).toHaveBeenCalledWith("get_chat_attachment_image", { conversationId: "chat1", id: "a1", full: true });
});
