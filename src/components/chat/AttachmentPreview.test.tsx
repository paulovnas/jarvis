import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { AttachmentPreview } from "./AttachmentPreview";
import type { Attachment } from "@/core/attachments";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const attachment: Attachment = { id: "image", conversationId: "chat", name: "screenshot.png", kind: "image", mime: "image/png", size: 30 };
it("mantém o skeleton até o navegador carregar a imagem", async () => {
  vi.mocked(invoke).mockResolvedValue("data:image/png;base64,cGljdHVyZQ==");
  render(<AttachmentPreview attachment={attachment} />);
  expect(screen.getByRole("status", { name: "Carregando imagem" })).toBeVisible();
  const image = await screen.findByRole("img", { name: attachment.name });
  expect(screen.getByRole("status", { name: "Carregando imagem" })).toBeVisible();
  fireEvent.load(image);
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});
it("mantém o nome completo acessível e limita o cartão do documento", () => {
  const name = `${"documento".repeat(40)}.txt`;
  render(<AttachmentPreview attachment={{ ...attachment, kind: "document", name }} />);
  expect(screen.getByTitle(name)).toHaveClass("truncate");
  expect(screen.getByTitle(name).closest(".w-52")).toHaveClass("max-w-full", "min-w-0");
});
