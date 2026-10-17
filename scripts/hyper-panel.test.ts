import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { expect, it, vi } from "vitest";

const script = readFileSync("src-tauri/src/openai_codex/charm_hyper/panel.js", "utf8");
const userId = "12345678-1234-4234-8234-123456789012";
const teamId = "23456789-1234-4234-8234-123456789012";
const session = { authenticated: true, user: { id: userId }, teams: [{ id: teamId, name: "My team" }] };
const dashboard = { now: 1792205393916, window_minutes: 300, subscribed: true, limited: true, known: true, available: 0, remaining: 78.851029874, cap: 100, allowance: 180, plan_name: "Test plan", returns_at: 1792207798000, refresh_hours: 168, refresh_at: 1792281600000 };

async function query(options: { session?: unknown; dashboard?: unknown; selection?: { teamId?: string; userId?: string }; origin?: string; oversized?: boolean } = {}) {
  const fetch = vi.fn(async (path: string, request: RequestInit) => {
    expect(request).toMatchObject({ credentials: "same-origin", redirect: "error", cache: "no-store", headers: { Accept: "application/json" } });
    return {
      ok: true,
      headers: { get: () => "application/json" },
      text: async () => options.oversized ? " ".repeat(65537) : JSON.stringify(path === "/auth/me" ? options.session ?? session : options.dashboard ?? dashboard),
    };
  });
  const result: unknown = await runInNewContext(script.replace("__JARVIS_HYPER_SELECTION__", JSON.stringify(options.selection ?? {})), {
    location: { origin: options.origin ?? "https://hyper.charm.land" }, fetch, AbortController, setTimeout, clearTimeout,
  });
  return { result, fetch };
}

it("reads only authenticated Hyper JSON and never returns cookies, tokens or unrelated dashboard data", async () => {
  const { result, fetch } = await query({ session: { ...session, cookie: "not-exported", access_token: "not-exported" }, dashboard: { ...dashboard, access_token: "not-exported", api_keys: ["not-exported"] } });
  expect(result).toEqual({ userId, teams: session.teams, teamId, dashboard });
  expect(fetch.mock.calls.map(([path]) => path)).toEqual(["/auth/me", `/teams/${teamId}/dashboard`]);
});

it("requires a login and preserves team selection when multiple teams exist", async () => {
  const loggedOut = await query({ session: { authenticated: false } });
  expect(loggedOut.result).toEqual({ error: "login_required" });
  expect(loggedOut.fetch).toHaveBeenCalledTimes(1);
  const teams = [...session.teams, { id: "34567890-1234-4234-8234-123456789012", name: "Other team" }];
  const choice = await query({ session: { ...session, teams } });
  expect(choice.result).toEqual({ userId, teams, teamId: null, dashboard: null });
  expect(choice.fetch).toHaveBeenCalledTimes(1);
});

it("refuses a changed account, a team outside that account and a page outside Hyper", async () => {
  const changedUser = await query({ selection: { userId: "other-user", teamId } });
  expect(changedUser.result).toEqual({ error: "different_user" });
  expect(changedUser.fetch).toHaveBeenCalledTimes(1);
  const changedTeam = await query({ selection: { userId, teamId: "other-team" } });
  expect(changedTeam.result).toEqual({ error: "different_team" });
  expect(changedTeam.fetch).toHaveBeenCalledTimes(1);
  const wrongPage = await query({ origin: "https://example.com" });
  expect(wrongPage.result).toEqual({ error: "wrong_origin" });
  expect(wrongPage.fetch).not.toHaveBeenCalled();
});

it("bounds dashboard session responses and returns a fixed failure instead of page content", async () => {
  const { result } = await query({ oversized: true });
  expect(result).toEqual({ error: "session_unavailable" });
});
