import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useClaudeRuntime } from "./use-claude-runtime";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

it("shares one metadata probe across selectors and refreshes all consumers explicitly", async () => {
  const first = { installed: true, authenticated: false, version: "2", error: null, models: [{ id: "default", name: "Padrão do CLI", description: "", reasoningLevels: [], defaultReasoning: null }] };
  vi.mocked(invoke).mockResolvedValue(first);
  const dormant = renderHook(() => useClaudeRuntime(false));
  expect(invoke).not.toHaveBeenCalled();
  const selectors = renderHook(() => [useClaudeRuntime(), useClaudeRuntime()]);
  await waitFor(() => expect(selectors.result.current[0].data).toEqual(first));
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_runtime");
  expect(dormant.result.current.data).toEqual(first);
  const connected = { ...first, authenticated: true, email: "person@example.test" };
  vi.mocked(invoke).mockResolvedValue(connected);
  await act(async () => { await selectors.result.current[0].refresh(); });
  expect(invoke).toHaveBeenLastCalledWith("refresh_claude_runtime");
  expect(selectors.result.current[1].data).toEqual(connected);
  expect(dormant.result.current.data).toEqual(connected);
});
