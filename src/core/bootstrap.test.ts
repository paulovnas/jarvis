import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { coreFixture } from "@/test/core-fixtures";
import { emptyLibrary } from "@/test/library-fixtures";
import {
  loadAppBootstrap,
} from "./bootstrap";
import { bootstrapPercent, initialBootstrapProgress, type BootstrapProgressEvent } from "./bootstrap-state";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const account = {
  alias: "openai-codex-paulo",
  providerKind: "openai-codex",
  enabled: true,
  createdAt: 1,
  email: "paulo@example.com",
  accountType: "personal",
  modelsAvailable: true,
  models: [{ id: "gpt-5.6", name: "GPT 5.6", reasoningLevels: ["medium"], defaultReasoningLevel: "medium" }],
};
const skills = {
  includeAgents: false,
  directory: "/home/.jarvis/skills",
  warnings: [],
  skills: [{
    id: "jarvis/example",
    name: "example",
    description: "Example skill",
    origin: "jarvis",
    path: "/home/.jarvis/skills/example",
    removalPath: "/home/.jarvis/skills/example",
    linked: false,
    enabled: true,
    automatic: true,
    source: "owner/repository",
    marketplaceId: "owner/repository/example",
    updateAvailable: false,
    updateError: null,
  }],
};

describe("application bootstrap", () => {
  beforeEach(() => invokeMock.mockReset());

  it("loads only the persisted configuration before onboarding", async () => {
    invokeMock.mockResolvedValue({ onboardingCompleted: false });
    const events: BootstrapProgressEvent[] = [];

    const result = await loadAppBootstrap((event) => events.push(event));

    expect(result).toEqual({ config: { onboardingCompleted: false }, resources: null });
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("get_app_config");
    expect(events[events.length - 1]).toMatchObject({ id: "configuration", progress: 1, status: "complete" });
  });

  it("preloads resources without waiting for Marketplace update discovery", async () => {
    invokeMock.mockImplementation((command, args) => {
      if (command === "get_app_config") return Promise.resolve({ onboardingCompleted: true });
      if (command === "check_core_updates") return Promise.resolve(coreFixture());
      if (command === "list_provider_accounts") return Promise.resolve([account]);
      if (command === "get_provider_usage") return Promise.resolve({
        alias: (args as { alias: string }).alias,
        fetchedAt: Date.now(),
        email: account.email,
        plan: "plus",
        windows: [],
        error: null,
        resetCredits: null,
      });
      if (command === "list_skills") return Promise.resolve(skills);
      if (command === "get_library_snapshot") return Promise.resolve(emptyLibrary());
      return Promise.resolve([]);
    });
    const state = initialBootstrapProgress();

    const result = await loadAppBootstrap((event) => { state[event.id] = event; });

    expect(result.resources?.loaded).toEqual({ core: true, skills: true, accounts: true, usage: true, library: true });
    expect(result.resources?.checked).toEqual({ core: true, skills: false });
    expect(result.resources?.skills?.skills[0].updateAvailable).toBe(false);
    expect(result.resources?.usageByAlias[account.alias]?.data?.plan).toBe("plus");
    expect(result.resources?.warnings).toEqual([]);
    expect(bootstrapPercent(state)).toBe(100);
    expect(invokeMock.mock.calls.filter(([command]) => command === "check_core_updates")).toHaveLength(1);
    expect(invokeMock.mock.calls.filter(([command]) => command === "check_skill_updates")).toHaveLength(0);
    expect(invokeMock.mock.calls.map(([command]) => command)).not.toContain(undefined);
  });

  it("keeps local Core data available when the remote version check fails", async () => {
    invokeMock.mockImplementation((command) => {
      if (command === "check_core_updates") return Promise.reject(new Error("offline"));
      if (command === "get_core_status") return Promise.resolve(coreFixture());
      if (command === "list_provider_accounts") return Promise.resolve([]);
      if (command === "list_skills") return Promise.resolve({ ...skills, skills: [] });
      if (command === "get_library_snapshot") return Promise.resolve(emptyLibrary());
      return Promise.resolve([]);
    });

    const result = await loadAppBootstrap(() => {}, { onboardingCompleted: true });

    expect(result.resources?.core).toEqual(coreFixture());
    expect(result.resources?.checked.core).toBe(false);
    expect(result.resources?.warnings).toContain("Não foi possível verificar as atualizações do Core.");
    expect(invokeMock.mock.calls.map(([command]) => command)).not.toContain(undefined);
  });

  it("continues with Core when every non-Core preload fails", async () => {
    invokeMock.mockImplementation((command) => {
      if (command === "check_core_updates") return Promise.resolve(coreFixture());
      if (["list_provider_accounts", "list_skills", "get_library_snapshot"].includes(command)) {
        return Promise.reject(new Error(`${command} unavailable`));
      }
      return Promise.resolve([]);
    });

    const result = await loadAppBootstrap(() => {}, { onboardingCompleted: true });

    expect(result.resources?.core).toEqual(coreFixture());
    expect(result.resources?.loaded).toEqual({ core: true, skills: false, accounts: false, usage: false, library: false });
    expect(result.resources?.warnings).toEqual([
      "Não foi possível carregar os provedores conectados.",
      "Não foi possível carregar as skills instaladas.",
      "Não foi possível pré-carregar os workspaces.",
    ]);
    expect(invokeMock.mock.calls.map(([command]) => command).filter(Boolean).sort()).toEqual([
      "check_core_updates",
      "get_library_snapshot",
      "list_provider_accounts",
      "list_skills",
    ]);
  });
});
