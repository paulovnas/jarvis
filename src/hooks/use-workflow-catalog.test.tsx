import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useWorkflowCatalog } from "./use-workflow-catalog";
import { customAgent, customCatalog } from "@/test/workflow-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });
it("serializes saves, sends the editor revision and keeps a newer catalog after delayed acknowledgements", async () => {
  let acknowledge: (value: unknown) => void = () => {};
  let latest = customCatalog;
  vi.mocked(invoke).mockImplementation(async command => command === "get_workflow_catalog" ? latest : new Promise(resolve => { acknowledge = resolve; }));
  const { result } = renderHook(() => useWorkflowCatalog());
  await waitFor(() => expect(result.current.data?.revision).toBe(2));
  let saved: Promise<boolean> = Promise.resolve(false);
  act(() => { saved = result.current.mutate({ kind: "save_agent", agent: customAgent }, 2); });
  expect(await result.current.mutate({ kind: "save_agent", agent: customAgent }, 2)).toBe(false);
  expect(invoke).toHaveBeenCalledWith("mutate_workflow_catalog", { revision: 2, mutation: { kind: "save_agent", agent: customAgent } });
  latest = { ...customCatalog, revision: 4 };
  await act(() => result.current.refresh());
  await act(async () => { acknowledge({ ...customCatalog, revision: 3 }); await saved; });
  expect(result.current.data?.revision).toBe(4);
  expect(result.current.saving).toBe(false);
});
it("surfaces unreadable catalogs without presenting an empty successful catalog", async () => {
  vi.mocked(invoke).mockRejectedValue({ message: "Catálogo inválido" });
  const { result } = renderHook(() => useWorkflowCatalog());
  await waitFor(() => expect(result.current.error).toBe("Catálogo inválido"));
  expect(result.current.data).toBeNull();
});
