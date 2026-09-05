import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { populatedLibrary } from "@/test/library-fixtures";
import { useLibrary } from "./use-library";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
describe("Library generated title refresh", () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });
  it("refreshes a generated title received while another library mutation is pending", async () => {
    let handler: EventCallback<unknown> | undefined;
    vi.mocked(listen).mockImplementation(async (_event, callback) => { handler = callback; return () => {}; });
    const initial = populatedLibrary();
    const renamed = populatedLibrary(); renamed.projects[0].name = "Renamed";
    const latest = populatedLibrary(); latest.projects[0].name = "Renamed"; latest.conversations[0].title = "Título gerado";
    let resolve!: (value: unknown) => void;
    let reads = 0;
    vi.mocked(invoke).mockImplementation(command => command === "get_library_snapshot"
      ? Promise.resolve(reads++ === 0 ? initial : latest)
      : new Promise(done => { resolve = done; }));
    const { result } = renderHook(() => useLibrary());
    await waitFor(() => expect(result.current.snapshot).toEqual(initial));
    let rename!: Promise<boolean>;
    act(() => { rename = result.current.renameProject("p1", "Renamed"); });
    await act(async () => { handler?.({ event:"library:changed", id:1, payload:"c1" }); });
    expect(reads).toBe(1);
    await act(async () => { resolve(renamed); await rename; });
    await waitFor(() => expect(result.current.snapshot?.conversations[0].title).toBe("Título gerado"));
    expect(result.current.snapshot?.projects[0].name).toBe("Renamed");
  });
});
