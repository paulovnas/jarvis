import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import type { AgyRuntime } from "@/core/agy";
import { AgyUsage } from "./ProviderUsage";

const runtime = vi.hoisted(() => ({ data: null as AgyRuntime | null }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/hooks/use-agy-runtime", () => ({ useAgyRuntime: () => runtime }));
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  runtime.data = { installed: true, authenticated: true, version: "1.2.13", error: null, preferences: { enabled: true, showUsage: true, disabledModels: [] }, models: [] };
});

it("shows native CLI quota metadata without requesting direct-provider usage", async () => {
  const now = Date.now();
  vi.mocked(invoke).mockResolvedValue({ alias: "Antigravity CLI", email: null, plan: null, fetchedAt: now, error: null, resetCredits: null, windows: [{ id: "gemini", group: "Gemini", thirdParty: false, label: "Gemini Pro", durationSeconds: null, remainingPercent: 78, resetsAt: now + 60_000 }] });
  const user = userEvent.setup();
  render(<AgyUsage now={now} />);
  const button = await screen.findByRole("button", { name: "Limites de Antigravity CLI" });
  await waitFor(() => expect(button).toHaveTextContent("78%"));
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_agy_usage");
  await user.click(button);
  expect(await screen.findByText("Antigravity CLI · CLI local")).toBeVisible();
  expect(screen.getByRole("progressbar", { name: "Gemini Gemini Pro restante" })).toHaveAttribute("aria-valuenow", "78");
});

it("reports unavailable quota truthfully and respects the optional status preference", async () => {
  vi.mocked(invoke).mockResolvedValue({ alias: "Antigravity CLI", email: null, plan: null, fetchedAt: null, error: "O CLI não informou cotas.", resetCredits: null, windows: [] });
  const user = userEvent.setup();
  const { rerender } = render(<AgyUsage now={Date.now()} />);
  await user.click(await screen.findByRole("button", { name: "Limites de Antigravity CLI" }));
  expect(await screen.findByText("O CLI não informou cotas.")).toBeVisible();
  expect(screen.getByText("Limites indisponíveis")).toBeVisible();
  runtime.data!.preferences!.showUsage = false;
  rerender(<AgyUsage now={Date.now()} />);
  expect(screen.queryByRole("button", { name: "Limites de Antigravity CLI" })).not.toBeInTheDocument();
});

it.each(["disabled", "missing", "login"])("does not query cotas when the CLI is %s", state => {
  if (state === "disabled") runtime.data!.preferences!.enabled = false;
  if (state === "missing") runtime.data!.installed = false;
  if (state === "login") runtime.data!.authenticated = false;
  render(<AgyUsage now={Date.now()} />);
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalled();
});
