import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ClaudeRuntime } from "@/core/executors";
import type { AccountUsage } from "@/core/provider-usage";
import { StatusBar } from "./StatusBar";

const local = vi.hoisted(() => ({ runtime: null as ClaudeRuntime | null }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => ({ data: local.runtime }) }));
vi.mock("./AppUpdate", () => ({ AppUpdate: () => null }));
vi.mock("./ResourceUpdates", () => ({ ResourceUpdates: () => null }));

const report = (): AccountUsage => ({ alias: "Claude Code", fetchedAt: Date.now(), email: "local@example.test", plan: "max", error: null, resetCredits: null, windows: [
  { id: "five_hour", group: "Claude", label: "5h", durationSeconds: 18_000, remainingPercent: 73, resetsAt: Date.now() + 3_600_000, thirdParty: false },
  { id: "weekly", group: "Claude", label: "7d", durationSeconds: 604_800, remainingPercent: 46, resetsAt: Date.now() + 86_400_000, thirdParty: false },
] });
beforeEach(() => {
  local.runtime = { installed: true, authenticated: true, version: "2", error: null, models: [], preferences: { enabled: true, showUsage: true, disabledModels: [] } };
  vi.mocked(invoke).mockReset().mockResolvedValue(report());
});

it("shows native Claude limits alongside providers with the same remaining quota and reset details", async () => {
  const user = userEvent.setup();
  render(<StatusBar />);
  const button = await screen.findByRole("button", { name: "Limites de Claude Code" });
  await waitFor(() => expect(button).toHaveTextContent("73%"));
  expect(button).toHaveTextContent("46%");
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_usage");
  await user.click(button);
  expect(await screen.findByText("Max")).toBeVisible();
  expect(screen.getByRole("progressbar", { name: "Claude 5h restante" })).toHaveAttribute("aria-valuenow", "73");
  expect(screen.getByRole("progressbar", { name: "Claude 7d restante" })).toHaveAttribute("aria-valuenow", "46");
  expect(screen.getAllByText(/Renova em/)).toHaveLength(2);
});

it("keeps confirmed quotas marked stale after a failed refresh and explains unavailable modes", async () => {
  const user = userEvent.setup();
  const { rerender } = render(<StatusBar />);
  const button = await screen.findByRole("button", { name: "Limites de Claude Code" });
  await waitFor(() => expect(button).toHaveTextContent("73%"));
  vi.mocked(invoke).mockRejectedValueOnce(new Error("offline"));
  act(() => window.dispatchEvent(new Event("focus")));
  await waitFor(() => expect(screen.getByLabelText("Limites desatualizados")).toBeInTheDocument());
  expect(button).toHaveTextContent("73%");
  vi.mocked(invoke).mockResolvedValue({ ...report(), windows: [], fetchedAt: null, error: "A conexão atual do Claude Code não informa cotas de assinatura." });
  act(() => window.dispatchEvent(new Event("focus")));
  await waitFor(() => expect(button).not.toHaveTextContent("73%"));
  expect(button).not.toHaveTextContent("0%");
  await user.click(button);
  expect(await screen.findByText("A conexão atual do Claude Code não informa cotas de assinatura.")).toBeVisible();
  local.runtime = { ...local.runtime!, preferences: { enabled: true, showUsage: false, disabledModels: [] } };
  rerender(<StatusBar />);
  expect(screen.queryByRole("button", { name: "Limites de Claude Code" })).not.toBeInTheDocument();
});

it.each(["uninstalled", "signed-out", "disabled", "hidden"])("does not poll Claude quotas when %s", async mode => {
  local.runtime = { ...local.runtime!, installed: mode !== "uninstalled", authenticated: mode !== "signed-out", preferences: { enabled: mode !== "disabled", showUsage: mode !== "hidden", disabledModels: [] } };
  render(<StatusBar />);
  await act(async () => {});
  expect(screen.queryByRole("button", { name: "Limites de Claude Code" })).not.toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalled();
});
