import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { VideoPreview } from "@/core/project-files";
import { VideoViewer } from "./VideoViewer";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const file: VideoPreview = { path: "renders/demo.mp4", absolutePath: "/project/renders/demo.mp4", url: "asset://localhost/project/renders/demo.mp4", size: 4000, mime: "video/mp4" };
beforeEach(() => { vi.mocked(invoke).mockReset().mockResolvedValue(false); vi.mocked(toast.success).mockClear(); vi.mocked(toast.error).mockClear(); });

it("offers save and external playback after a codec or file playback error", async () => {
  const user = userEvent.setup();
  render(<VideoViewer projectId="project-1" file={file} />);
  fireEvent.error(screen.getByLabelText("Vídeo demo.mp4"));
  expect(screen.getByRole("alert")).toHaveTextContent("O formato ou codec pode não ser compatível");
  expect(screen.queryByRole("status", { name: "Carregando vídeo" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Salvar vídeo" }));
  expect(invoke).toHaveBeenCalledWith("save_project_video", { projectId: "project-1", path: file.path });
  expect(toast.success).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Abrir no aplicativo padrão" }));
  expect(invoke).toHaveBeenCalledWith("open_project_video", { projectId: "project-1", path: file.path });
});

it("reports save failures and releases the actions for retry", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockRejectedValueOnce(new Error("Disk full"));
  render(<VideoViewer projectId="project-1" file={file} />);
  await user.click(screen.getByRole("button", { name: "Salvar vídeo" }));
  expect(toast.error).toHaveBeenCalledWith("Não foi possível salvar o vídeo");
  await waitFor(() => expect(screen.getByRole("button", { name: "Salvar vídeo" })).toBeEnabled());
  vi.mocked(invoke).mockResolvedValueOnce(true);
  await user.click(screen.getByRole("button", { name: "Salvar vídeo" }));
  expect(toast.success).toHaveBeenCalledWith("Vídeo salvo");
});
