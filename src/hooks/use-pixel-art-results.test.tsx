import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { expect, it, vi } from "vitest";
import { usePixelArtResults } from "./use-pixel-art-results";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

it("restores saved jobs and manual exports without regressing newer live events or mixing conversations", async () => {
  let resolve!: (value: unknown) => void;
  vi.mocked(invoke).mockImplementation(() => new Promise(done => { resolve = done; }));
  let notify!: EventCallback<unknown>;
  const unlisten = vi.fn();
  vi.mocked(listen).mockImplementation(async (_event, callback) => { notify = callback; return unlisten; });
  const saved = { kind: "pixel_art", jobId: "job", conversationId: "chat", revision: 2, action: "generate", status: "completed", phase: "Arte salva" };
  const { result, unmount } = renderHook(() => usePixelArtResults("chat"));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_pixel_art_jobs", { conversationId: "chat" }));
  await act(async () => {
    notify({ event: "pixel-art-progress", id: 1, payload: { ...saved, revision: 3, phase: "GIF exportado" } });
    resolve([saved, { ...saved, jobId: "manual", action: "export", manual: true }, { ...saved, jobId: "other", conversationId: "different-chat" }]);
  });
  await waitFor(() => expect(Object.keys(result.current)).toEqual(["job", "manual"]));
  expect(result.current.job.phase).toBe("GIF exportado");
  expect(result.current.manual.manual).toBe(true);
  expect(invoke).toHaveBeenCalledWith("get_pixel_art_jobs", { conversationId: "chat" });
  unmount();
  await waitFor(() => expect(unlisten).toHaveBeenCalledOnce());
});

it("installs the event subscription before loading receipts so completion cannot fall into a startup gap", async () => {
  let subscribed!: (unlisten: () => void) => void;
  vi.mocked(invoke).mockReset().mockResolvedValue([]);
  vi.mocked(listen).mockImplementation(() => new Promise(resolve => { subscribed = resolve; }));
  const { unmount } = renderHook(() => usePixelArtResults("chat"));
  expect(invoke).not.toHaveBeenCalled();
  await act(async () => subscribed(vi.fn()));
  expect(invoke).toHaveBeenCalledWith("get_pixel_art_jobs", { conversationId: "chat" });
  unmount();
});
