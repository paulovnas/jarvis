import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { AttachmentPreview } from "./AttachmentPreview";
import type { Attachment } from "@/core/attachments";
import { writeClipboardImage, writeClipboardText } from "@/core/clipboard";
import { TextContextMenu } from "../TextContextMenu";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/core/clipboard", () => ({ readClipboardText: vi.fn(), writeClipboardText: vi.fn(), writeClipboardImage: vi.fn() }));
const attachment: Attachment = { id: "image", conversationId: "chat", name: "screenshot.png", kind: "image", mime: "image/png", size: 30 };
it("copies the expanded attachment through the global image context menu", async () => {
  const user = userEvent.setup();
  const thumbnail = "data:image/png;base64,dGh1bWI=";
  const full = "data:image/png;base64,ZnVsbA==";
  vi.mocked(invoke).mockImplementation(async (_command, args) => args && "full" in args && args.full ? full : thumbnail);
  vi.mocked(writeClipboardImage).mockReset().mockResolvedValue(undefined);
  vi.mocked(writeClipboardText).mockReset();
  render(<TextContextMenu><div><AttachmentPreview attachment={attachment} /></div></TextContextMenu>);
  fireEvent.load(await screen.findByRole("img", { name: attachment.name }));
  await user.click(screen.getByRole("button", { name: `Ampliar ${attachment.name}` }));
  const dialog = await screen.findByRole("dialog", { name: attachment.name });
  const image = await within(dialog).findByRole("img", { name: attachment.name });
  fireEvent.load(image);
  fireEvent.contextMenu(image);
  expect(screen.queryByRole("menuitem", { name: "Copiar" })).not.toBeInTheDocument();
  expect(screen.queryByRole("menuitem", { name: "Colar" })).not.toBeInTheDocument();
  await user.click(await screen.findByRole("menuitem", { name: "Copiar imagem" }));
  expect(writeClipboardImage).toHaveBeenCalledWith(full);
  expect(writeClipboardText).not.toHaveBeenCalled();
});
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
  expect(screen.getByText(name)).toHaveClass("truncate");
  expect(screen.getByText(name).closest(".w-52")).toHaveClass("max-w-full", "min-w-0");
});
