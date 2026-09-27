import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, expect, it, vi } from "vitest";
import { BootstrapResourcesProvider } from "./BootstrapResourcesProvider";
import { useBootstrapResources } from "@/hooks/use-bootstrap-resources";
import type { BootstrapResources } from "@/core/bootstrap";
import { coreFixture } from "@/test/core-fixtures";
import { emptyLibrary } from "@/test/library-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });
const account = { alias: "openai-codex-test", enabled: true, providerKind: "openai-codex", createdAt: 1, email: null, accountType: "personal" as const, modelsAvailable: true, modelsStale: true, models: [{ id: "cached", name: "Cached", reasoningLevels: [], defaultReasoningLevel: null }] };
function initial(): BootstrapResources {
  return { core: coreFixture(), skills: null, library: emptyLibrary(), accounts: [account], usageByAlias: {}, checked: { core: false, skills: false }, loaded: { core: true, skills: false, library: true, accounts: true, usage: false }, warnings: [] };
}
function Probe() {
  const context = useBootstrapResources()!;
  const current = context.resources.accounts[0];
  return <><span>{current?.models[0]?.id}</span><span>{context.resources.checked.core ? "checked" : "local"}</span><span>{current?.enabled ? "enabled" : "disabled"}</span><button onClick={() => context.updateAccounts([{ ...current, enabled: false }])}>Disable</button></>;
}

it("keeps local resources visible during stalled reads and refreshes them after reconnection", async () => {
  vi.useFakeTimers();
  let late!: (value: unknown) => void;
  const stalled = new Promise(resolve => { late = resolve; });
  vi.mocked(invoke).mockReturnValue(stalled);
  const view = render(<BootstrapResourcesProvider initial={initial()}><Probe /></BootstrapResourcesProvider>);
  expect(screen.getByText("cached")).toBeVisible();
  await act(() => vi.advanceTimersByTimeAsync(100_001));
  expect(screen.getByText("cached")).toBeVisible();
  expect(screen.getByText("local")).toBeVisible();
  vi.mocked(invoke).mockImplementation(command => Promise.resolve(command === "check_core_updates" ? coreFixture() : [{ ...account, modelsStale: false, models: [{ ...account.models[0], id: "fresh" }] }]));
  await act(async () => { window.dispatchEvent(new Event("online")); });
  expect(screen.getByText("fresh")).toBeVisible();
  expect(screen.getByText("checked")).toBeVisible();
  await act(async () => { late([account]); });
  expect(screen.getByText("fresh")).toBeVisible();
  view.unmount();
});

it("does not overwrite account changes with an older background response", async () => {
  let finish!: (value: unknown) => void;
  vi.mocked(invoke).mockImplementation(command => command === "check_core_updates" ? Promise.resolve(coreFixture()) : new Promise(resolve => { finish = resolve; }));
  const view = render(<BootstrapResourcesProvider initial={initial()}><Probe /></BootstrapResourcesProvider>);
  fireEvent.click(screen.getByRole("button", { name: "Disable" }));
  await act(async () => { finish([account]); });
  expect(screen.getByText("disabled")).toBeVisible();
  view.unmount();
});

it("keeps recovery pending while a native Core check is already running", async () => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockImplementation(command => Promise.resolve(command === "check_core_updates" ? { ...coreFixture(), checking: true } : [{ ...account, modelsStale: false }]));
  const view = render(<BootstrapResourcesProvider initial={initial()}><Probe /></BootstrapResourcesProvider>);
  await act(() => vi.advanceTimersByTimeAsync(0));
  expect(screen.getByText("local")).toBeVisible();
  vi.mocked(invoke).mockResolvedValue(coreFixture());
  await act(async () => { window.dispatchEvent(new Event("online")); });
  expect(screen.getByText("checked")).toBeVisible();
  view.unmount();
});
